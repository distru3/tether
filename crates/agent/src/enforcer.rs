//! Enforcement: translate the limits engine's verdicts into block state.
//!
//! On every evaluation tick the enforcer:
//!
//! 1. Looks up the focused app's record (category + tags).
//! 2. Asks the limits engine what to do (`Allow`/`Warn`/`Block`).
//! 3. On `Block`: records the block in `block_state` so the overlay owner (the
//!    session helper) knows to cover the app. On `Allow`/`Warn`: clears the
//!    block, so a PIN override lifts it live.
//! 4. On day rollover: clears yesterday's blocks, because every budget resets
//!    at the day boundary.
//!
//! The agent never touches the app's processes. M2.5 moved blocking from a
//! process freeze to a non-invasive overlay: the session helper draws a
//! topmost window over the blocked app and blocks its input, which works even
//! for anti-cheat-protected or elevated processes that `NtSuspendProcess`
//! could never touch. `block_state` is the sole source of truth the overlay
//! polls.

use st_core::category::CategoryKind;
use st_core::daykey::DayKey;
use st_core::limits::{Decision, LimitEngine};
use st_core::model::{AppKey, SubjectRef};
use st_storage::Db;

pub struct Enforcer {
    /// Last day we evaluated. A new day means every budget resets, so any
    /// blocks from the previous day must be cleared.
    last_day: Option<DayKey>,
}

impl Enforcer {
    pub fn new() -> Self {
        Self { last_day: None }
    }

    /// Evaluate and act on the focused app. Returns the verdict for logging.
    ///
    /// `window_key` is the focused app. If it is `None` (desktop, locked,
    /// nothing focused) nothing is evaluated but day rollover still happens.
    pub fn tick(
        &mut self,
        db: &mut Db,
        engine: &LimitEngine,
        window_key: Option<&AppKey>,
        today: DayKey,
        weekday: usize,
    ) -> anyhow::Result<()> {
        if self.last_day != Some(today) {
            self.handle_day_rollover(db)?;
            self.last_day = Some(today);
        }

        let Some(window_key) = window_key else {
            return Ok(());
        };

        // Not every focused window is a tracked app (e.g. the lock screen is
        // filtered upstream); resolve it via storage, then evaluate.
        let Some(app_id) = db.app_id_for_key(window_key)? else {
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
                db.set_block(subject, "limit", chrono::Utc::now(), None)?;
            }
            Decision::Allow { .. } | Decision::Warn { .. } => {
                db.clear_block(subject)?;
            }
        }
        Ok(())
    }

    /// On a new day, clear yesterday's blocks. Budgets reset at the boundary,
    /// so blocks from the previous day must not survive it. The overlay owner
    /// will simply stop showing an overlay once the block is gone.
    fn handle_day_rollover(&mut self, db: &mut Db) -> anyhow::Result<()> {
        let blocked = db.blocked_subjects()?;
        for (subject, _reason) in blocked {
            let _ = db.clear_block(subject);
        }
        Ok(())
    }

    /// Whether the app may ever be blocked, from its primary category kind.
    fn blockable(&self, db: &Db, primary_category: i64) -> anyhow::Result<bool> {
        Ok(!matches!(
            db.category_kind(primary_category)?,
            Some(CategoryKind::NeverBlock)
        ))
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

    fn engine_for(target: st_core::limits::LimitTarget) -> LimitEngine {
        LimitEngine::with_default_warnings(vec![Limit::new(1, target, 0)])
    }

    fn steam_key() -> AppKey {
        AppKey::windows_exe("C:\\games\\steam\\steam.exe")
    }

    fn tick_blocking(
        enforcer: &mut Enforcer,
        db: &mut Db,
        key: &AppKey,
        day: DayKey,
        weekday: usize,
        target: st_core::limits::LimitTarget,
    ) {
        let engine = engine_for(target);
        enforcer
            .tick(db, &engine, Some(key), day, weekday)
            .expect("tick");
    }

    #[test]
    fn exhausted_limit_records_a_block() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        let day = DayKey(20260820);

        let app = seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");
        snapshot_usage(&mut db, app, 60 * 60);

        let mut enforcer = Enforcer::new();
        tick_blocking(
            &mut enforcer,
            &mut db,
            &steam_key(),
            day,
            3,
            st_core::limits::LimitTarget::Category(games),
        );

        assert!(db.is_blocked(SubjectRef::App(app)).expect("blocked"));
    }

    #[test]
    fn an_override_lifts_the_block_live() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        let day = DayKey(20260820);

        let app = seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");
        snapshot_usage(&mut db, app, 10 * 60);

        let mut enforcer = Enforcer::new();
        tick_blocking(
            &mut enforcer,
            &mut db,
            &steam_key(),
            day,
            3,
            st_core::limits::LimitTarget::Category(games),
        );
        assert!(db.is_blocked(SubjectRef::App(app)).expect("blocked"));

        // A +15 override lifts the budget -> next tick must clear the block.
        db.grant_override(
            &st_core::limits::LimitTarget::Category(games),
            day,
            15 * 60,
            chrono::Utc::now(),
            Some("test"),
        )
        .expect("override");
        tick_blocking(
            &mut enforcer,
            &mut db,
            &steam_key(),
            day,
            3,
            st_core::limits::LimitTarget::Category(games),
        );
        assert!(!db.is_blocked(SubjectRef::App(app)).expect("unblocked"));
    }

    #[test]
    fn never_block_apps_are_never_blocked() {
        let mut db = Db::open_in_memory().expect("db");
        let day = DayKey(20260820);

        let app = seed_app(&mut db, "C:\\code.exe", "development");
        snapshot_usage(&mut db, app, 99 * 60);

        let mut enforcer = Enforcer::new();
        // A total budget of zero would block any blockable app.
        tick_blocking(
            &mut enforcer,
            &mut db,
            &AppKey::windows_exe("C:\\code.exe"),
            day,
            3,
            st_core::limits::LimitTarget::Total,
        );

        assert!(!db.is_blocked(SubjectRef::App(app)).expect("not blocked"));
    }

    #[test]
    fn blocks_are_cleared_on_day_rollover() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        let day = DayKey(20260820);

        let app = seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");
        snapshot_usage(&mut db, app, 60 * 60);

        let mut enforcer = Enforcer::new();
        tick_blocking(
            &mut enforcer,
            &mut db,
            &steam_key(),
            day,
            3,
            st_core::limits::LimitTarget::Category(games),
        );
        assert!(db.is_blocked(SubjectRef::App(app)).expect("blocked"));

        // Next day: everything is unblocked.
        let engine = engine_for(st_core::limits::LimitTarget::Category(games));
        enforcer
            .tick(&mut db, &engine, None, DayKey(20260821), 4)
            .expect("tick");
        assert!(!db.is_blocked(SubjectRef::App(app)).expect("cleared"));
    }

    #[test]
    fn a_zero_limit_blocks_immediately_without_usage() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        let day = DayKey(20260820);

        seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");
        let mut enforcer = Enforcer::new();
        tick_blocking(
            &mut enforcer,
            &mut db,
            &steam_key(),
            day,
            3,
            st_core::limits::LimitTarget::Category(games),
        );

        let blocked = db.blocked_subjects().expect("list");
        assert_eq!(blocked.len(), 1);
    }
}
