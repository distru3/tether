//! Enforcement: translate the limits engine's verdicts into frozen processes.
//!
//! This is the M2 heart of the app. On every evaluation tick the enforcer:
//!
//! 1. Looks up the focused app's record (category + tags).
//! 2. Asks the limits engine what to do (`Allow`/`Warn`/`Block`).
//! 3. On `Block`: freezes the app's whole process tree and records it in
//!    `block_state`. On `Allow`/`Warn`: thaws anything it froze and clears the
//!    block, so a PIN override lifts a block live.
//! 4. On day rollover: thaws everything and clears yesterday's blocks, because
//!    every budget resets at the day boundary.
//!
//! Freeze-instead-of-kill is deliberate (see `enforce-win`): a frozen app keeps
//! its unsaved work and can be resumed the moment the limit lifts.

use std::collections::HashMap;

use st_core::category::CategoryKind;
use st_core::daykey::DayKey;
use st_core::limits::{Decision, LimitEngine};
use st_core::model::{AppKey, SubjectRef};
use st_core::platform::ProcessController;
use st_storage::Db;

/// Tracks which PIDs the agent itself froze, so it never double-freezes (which
/// `NtSuspendProcess` treats as an error) and always thaws exactly what it
/// froze when the block lifts.
pub struct Enforcer {
    /// App key -> pids we froze. Keys are cloned so we can thaw after a row is
    /// deleted; values are the PIDs from `find_processes`.
    frozen: HashMap<AppKey, Vec<u32>>,
    /// Last day we evaluated. A new day means every budget resets, so anything
    /// we froze must be thawed.
    last_day: Option<DayKey>,
}

impl Enforcer {
    pub fn new() -> Self {
        Self {
            frozen: HashMap::new(),
            last_day: None,
        }
    }

    /// Evaluate and act on the focused app. Returns the verdict for logging.
    ///
    /// `window_key` is the focused app. If it is `None` (desktop, locked,
    /// nothing focused) nothing is evaluated but day rollover still happens.
    pub fn tick(
        &mut self,
        db: &mut Db,
        processes: &mut dyn ProcessController,
        engine: &LimitEngine,
        window_key: Option<&AppKey>,
        today: DayKey,
        weekday: usize,
    ) -> anyhow::Result<()> {
        if self.last_day != Some(today) {
            self.handle_day_rollover(db, processes)?;
            self.last_day = Some(today);
        }

        let Some(window_key) = window_key else {
            return Ok(());
        };

        // Not every focused window is a tracked app (e.g. the lock screen is
        // filtered upstream); resolve it via storage, then evaluate.
        let Some(app_id) = self.resolve_app_id(db, window_key)? else {
            return Ok(());
        };
        let Some(record) = db.app_record(app_id)? else {
            return Ok(());
        };

        let blockable = self.blockable(db, record.primary_category)?;
        let snapshot = db.day_snapshot(today)?;
        let decision = engine.evaluate(
            record.id,
            &record.all_categories(),
            blockable,
            weekday,
            &snapshot,
        );

        let subject = SubjectRef::App(record.id);
        match decision {
            Decision::Block { binding } => {
                tracing::info!(app = %record.key, ?binding, "blocking app");
                self.block(db, processes, window_key, &record.key, subject)?;
            }
            Decision::Allow { .. } | Decision::Warn { .. } => {
                self.unblock(db, processes, window_key, &record.key, subject)?;
            }
        }
        Ok(())
    }

    /// Freeze the app's process tree and record the block, unless it is already
    /// frozen (so a repeated Block verdict is a no-op).
    fn block(
        &mut self,
        db: &mut Db,
        processes: &mut dyn ProcessController,
        key: &AppKey,
        record_key: &AppKey,
        subject: SubjectRef,
    ) -> anyhow::Result<()> {
        if self.frozen.contains_key(key) {
            return Ok(());
        }
        let pids = processes.find_processes(key).unwrap_or_default();
        for pid in &pids {
            if let Err(e) = processes.freeze(*pid) {
                tracing::warn!(pid, error = %e, "freeze failed (process may have exited)");
            }
        }
        if pids.is_empty() {
            tracing::warn!(app = %record_key, "blocked but no matching processes found");
        }
        self.frozen.insert(key.clone(), pids);

        // Record the block. It expires at the day boundary, so the rollover
        // handler in `tick` will clear it. The reason is always `limit` for M2.
        let expires = None;
        let _ = db.set_block(subject, "limit", chrono::Utc::now(), expires);
        Ok(())
    }

    /// Thaw anything we froze for this app and clear its block.
    fn unblock(
        &mut self,
        db: &mut Db,
        processes: &mut dyn ProcessController,
        key: &AppKey,
        _record_key: &AppKey,
        subject: SubjectRef,
    ) -> anyhow::Result<()> {
        if let Some(pids) = self.frozen.remove(key) {
            for pid in &pids {
                if let Err(e) = processes.thaw(*pid) {
                    tracing::debug!(pid, error = %e, "thaw failed (process may have exited)");
                }
            }
        }
        let _ = db.clear_block(subject);
        Ok(())
    }

    /// On a new day, thaw every PID we froze and clear all blocks. Budgets
    /// reset at the boundary, so yesterday's blocks must not survive it.
    fn handle_day_rollover(
        &mut self,
        db: &mut Db,
        processes: &mut dyn ProcessController,
    ) -> anyhow::Result<()> {
        if self.frozen.is_empty() {
            return Ok(());
        }
        for (_key, pids) in self.frozen.drain() {
            for pid in &pids {
                let _ = processes.thaw(*pid);
            }
        }
        if let Ok(blocked) = db.blocked_subjects() {
            for (subject, _reason) in blocked {
                let _ = db.clear_block(subject);
            }
        }
        Ok(())
    }

    /// Whether the app may ever be frozen, from its primary category kind.
    fn blockable(&self, db: &Db, primary_category: i64) -> anyhow::Result<bool> {
        Ok(!matches!(
            db.category_kind(primary_category)?,
            Some(CategoryKind::NeverBlock)
        ))
    }

    /// Resolve an app key to its database id, if it is tracked at all.
    fn resolve_app_id(&self, db: &Db, key: &AppKey) -> anyhow::Result<Option<i64>> {
        Ok(db.app_id_for_key(key)?)
    }
}

impl Default for Enforcer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use st_core::limits::Limit;
    use st_core::platform::{PlatformError, PlatformResult};
    use std::cell::RefCell;

    /// Records freeze/thaw calls so tests can assert exactly what happened.
    #[derive(Default)]
    struct FakeController {
        calls: RefCell<Vec<String>>,
        pids: RefCell<HashMap<String, Vec<u32>>>,
    }

    impl FakeController {
        fn track(&self, key: &str, pids: Vec<u32>) {
            self.pids.borrow_mut().insert(key.to_string(), pids);
        }
        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }
    }

    impl ProcessController for FakeController {
        fn find_processes(&mut self, key: &AppKey) -> PlatformResult<Vec<u32>> {
            Ok(self
                .pids
                .borrow()
                .get(key.basename())
                .cloned()
                .unwrap_or_default())
        }
        fn freeze(&mut self, pid: u32) -> PlatformResult<()> {
            self.calls.borrow_mut().push(format!("freeze {pid}"));
            Ok(())
        }
        fn thaw(&mut self, pid: u32) -> PlatformResult<()> {
            self.calls.borrow_mut().push(format!("thaw {pid}"));
            Ok(())
        }
        fn terminate(&mut self, _pid: u32) -> PlatformResult<()> {
            Err(PlatformError::Unsupported("tests"))
        }
        fn backend(&self) -> &'static str {
            "fake"
        }
    }

    fn seed_app(db: &mut Db, key: &str, category_slug: &str) -> i64 {
        let uncat = db.category_id("uncategorized").expect("uncat");
        let cat = db.category_id(category_slug).expect("category");
        let app_key = AppKey::windows_exe(key);
        let id = db
            .upsert_app(&app_key, key, None, uncat, chrono::Utc::now())
            .expect("app");
        db.set_app_categories(id, cat, &[], false)
            .expect("classify");
        id
    }

    fn snapshot_usage(db: &mut Db, app: i64, secs: i64) {
        let day = DayKey(20260820);
        db.record_interval(&st_core::model::UsageInterval {
            subject: SubjectRef::App(app),
            session_id: "s".into(),
            start: chrono::Utc::now(),
            end: chrono::Utc::now() + chrono::Duration::seconds(secs),
            day_key: day,
        })
        .expect("interval");
    }

    #[test]
    fn exhausted_limit_freezes_and_records_the_block() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        let day = DayKey(20260820);

        let app = seed_app(&mut db, "C:\\steam.exe", "games");
        snapshot_usage(&mut db, app, 60 * 60);

        let mut controller = FakeController::default();
        controller.track("steam.exe", vec![11, 12]);
        let mut enforcer = Enforcer::new();
        let engine = LimitEngine::with_default_warnings(vec![Limit::new(
            1,
            st_core::limits::LimitTarget::Category(games),
            0,
        )]);

        enforcer
            .tick(
                &mut db,
                &mut controller,
                &engine,
                Some(&AppKey::windows_exe("C:\\steam.exe")),
                day,
                3,
            )
            .expect("tick");

        assert_eq!(controller.calls(), vec!["freeze 11", "freeze 12"]);
        assert!(db.is_blocked(SubjectRef::App(app)).expect("blocked"));
    }

    #[test]
    fn once_frozen_the_block_is_a_noop_until_allow() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        let day = DayKey(20260820);

        let app = seed_app(&mut db, "C:\\steam.exe", "games");
        snapshot_usage(&mut db, app, 10 * 60);

        let mut controller = FakeController::default();
        controller.track("steam.exe", vec![11]);
        let mut enforcer = Enforcer::new();
        let engine = LimitEngine::with_default_warnings(vec![Limit::new(
            1,
            st_core::limits::LimitTarget::Category(games),
            0,
        )]);

        for _ in 0..3 {
            enforcer
                .tick(
                    &mut db,
                    &mut controller,
                    &engine,
                    Some(&AppKey::windows_exe("C:\\steam.exe")),
                    day,
                    3,
                )
                .expect("tick");
        }
        assert_eq!(
            controller.calls(),
            vec!["freeze 11"],
            "no double freeze, got {:?}",
            controller.calls()
        );

        // A +15 override lifts the budget -> next tick must thaw.
        let over = st_core::limits::LimitTarget::Category(games);
        db.grant_override(&over, day, 15 * 60, chrono::Utc::now(), Some("test"))
            .expect("override");
        enforcer
            .tick(
                &mut db,
                &mut controller,
                &engine,
                Some(&AppKey::windows_exe("C:\\steam.exe")),
                day,
                3,
            )
            .expect("tick");
        assert_eq!(
            controller.calls(),
            vec!["freeze 11", "thaw 11"],
            "override must lift the freeze"
        );
        assert!(!db.is_blocked(SubjectRef::App(app)).expect("unblocked"));
    }

    #[test]
    fn never_block_apps_are_never_frozen() {
        let mut db = Db::open_in_memory().expect("db");
        let day = DayKey(20260820);

        let app = seed_app(&mut db, "C:\\code.exe", "development");
        snapshot_usage(&mut db, app, 99 * 60);

        let mut controller = FakeController::default();
        controller.track("code.exe", vec![1]);
        let mut enforcer = Enforcer::new();
        // A total budget of zero would block any blockable app.
        let engine = LimitEngine::with_default_warnings(vec![Limit::new(
            1,
            st_core::limits::LimitTarget::Total,
            0,
        )]);

        enforcer
            .tick(
                &mut db,
                &mut controller,
                &engine,
                Some(&AppKey::windows_exe("C:\\code.exe")),
                day,
                3,
            )
            .expect("tick");

        assert!(controller.calls().is_empty(), "never block");
    }

    #[test]
    fn frozen_apps_are_thawed_on_day_rollover() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        let day = DayKey(20260820);

        let app = seed_app(&mut db, "C:\\steam.exe", "games");
        snapshot_usage(&mut db, app, 60 * 60);

        let mut controller = FakeController::default();
        controller.track("steam.exe", vec![11]);
        let mut enforcer = Enforcer::new();
        let engine = LimitEngine::with_default_warnings(vec![Limit::new(
            1,
            st_core::limits::LimitTarget::Category(games),
            0,
        )]);

        enforcer
            .tick(
                &mut db,
                &mut controller,
                &engine,
                Some(&AppKey::windows_exe("C:\\steam.exe")),
                day,
                3,
            )
            .expect("tick");
        assert_eq!(controller.calls(), vec!["freeze 11"]);

        // Next day: everything thaws.
        enforcer
            .tick(&mut db, &mut controller, &engine, None, DayKey(20260821), 4)
            .expect("tick");
        assert_eq!(controller.calls(), vec!["freeze 11", "thaw 11"]);
    }
}
