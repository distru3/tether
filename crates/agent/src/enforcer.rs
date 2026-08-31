//! Enforcement: translate the limits engine's verdicts into block state.
//!
//! On every evaluation tick the enforcer:
//!
//! 1. Thaws blocks whose `expires_utc` has passed.
//! 2. On a *genuine* day change observed across two ticks, clears whatever
//!    survived (every budget resets at the boundary).
//! 3. Looks up the focused app's record (category + tags).
//! 4. Asks the limits engine what to do (`Allow`/`Warn`/`Block`).
//! 5. On `Block`: records the block in `block_state` **with an expiry** -- the
//!    end of the local day, computed via [`DayKey::end_utc`] -- so the block
//!    dies on its own even if this process never ticks again. On
//!    `Allow`/`Warn`: clears the block, so a PIN override lifts it live.
//!
//! # Why expiry is persisted instead of trusting in-memory state
//!
//! An earlier design kept "which day are we on" only in RAM and wiped every
//! block on the first tick after start. That made restarting or crashing the
//! agent a universal block-lift button. Blocks now carry their own deadline in
//! the database; a freshly started enforcer reads what is already there and
//! leaves unexpired blocks alone. Rollover clearing fires only when *this
//! process* observed the previous day itself -- startup never wipes.
//!
//! The agent never touches the app's processes. M2.5 moved blocking from a
//! process freeze to a non-invasive overlay: the session helper draws a
//! topmost window over the blocked app and blocks its input, which works even
//! for anti-cheat-protected or elevated processes that `NtSuspendProcess`
//! could never touch. `block_state` is the sole source of truth the overlay
//! polls.

use chrono::{DateTime, Duration, Utc};
use st_core::category::CategoryKind;
use st_core::daykey::DayKey;
use st_core::limits::{Decision, LimitEngine};
use st_core::model::{AppKey, SubjectRef};
use st_storage::Db;

/// Fallback expiry when a [`DayKey`] cannot be resolved to an instant. A day
/// key produced by `from_utc` always resolves, so this only guards against a
/// hand-corrupted key arriving some other way; 25 hours is safely past any
/// real boundary without pinning the block forever.
const FALLBACK_EXPIRY: Duration = Duration::hours(25);

pub struct Enforcer {
    /// Last day *this process* evaluated. `None` means "no observation yet",
    /// i.e. we just started -- and starting must never clear anything.
    last_day: Option<DayKey>,
}

impl Enforcer {
    pub fn new() -> Self {
        Self { last_day: None }
    }

    /// Evaluate and act on the focused app. Returns the verdict for logging.
    ///
    /// `window_key` is the focused app (from local sampling or from session-
    /// helper reports). If it is `None` nothing is evaluated but expiry thawing
    /// and day rollover still happen.
    ///
    /// `now`, `tz_offset_secs` and `day_start_minutes` come from the caller's
    /// injected [`Clock`](st_core::clock::Clock) so tamper semantics stay
    /// testable, and match exactly how usage was bucketed into `today`.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        db: &mut Db,
        engine: &LimitEngine,
        window_key: Option<&AppKey>,
        active_focus_session: Option<&st_core::FocusSession>,
        now: DateTime<Utc>,
        tz_offset_secs: i32,
        day_start_minutes: i64,
    ) -> anyhow::Result<()> {
        let today = DayKey::from_utc(now, tz_offset_secs, day_start_minutes);

        self.thaw_expired_blocks(db, now)?;

        match self.last_day {
            // Freshly started: blocks loaded from the database stay. Wiping
            // here is precisely the fail-open restart bug this file exists to
            // prevent.
            None => {
                tracing::debug!(day = %today, "first evaluation after start; surviving blocks preserved")
            }
            // Genuine rollover observed across two of our own ticks: budgets
            // reset, so nothing may carry over.
            Some(previous) if previous != today => self.clear_all_blocks(db)?,
            Some(_) => {}
        }
        self.last_day = Some(today);

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

        let subject = SubjectRef::App(record.id);
        let blockable = self.blockable(db, record.primary_category)?;

        // 1. Check Allowlist: if allowlisted, never block via downtime or focus session
        let allowlisted = db.is_subject_allowlisted("app", record.id)?;

        // 2. Check Focus Session block
        let focus_active = active_focus_session
            .map(|f| f.is_active(now))
            .unwrap_or(false);
        if blockable && focus_active && !allowlisted {
            let expires = active_focus_session
                .map(|f| f.expires_utc)
                .unwrap_or(now + FALLBACK_EXPIRY);
            tracing::info!(app = %record.key, "blocking app due to active focus session");
            db.set_block(subject, "focus_session", now, Some(expires))?;
            return Ok(());
        }

        // 3. Check Downtime Schedules
        let minute_of_day =
            (now.timestamp().rem_euclid(86400) + tz_offset_secs as i64).rem_euclid(86400) / 60;
        let weekday_idx = today.weekday_index().unwrap_or(0);
        let schedules = db.list_schedules()?;
        let downtime_active = schedules.iter().any(|s| {
            let sched = st_core::DowntimeSchedule {
                id: s.id,
                name: s.name.clone(),
                weekday_mask: s.weekday_mask,
                start_minute: s.start_minute,
                end_minute: s.end_minute,
                enabled: s.enabled,
            };
            st_core::schedules::is_schedule_active(&sched, minute_of_day as u32, weekday_idx as u32)
        });

        if blockable && downtime_active && !allowlisted {
            let expires = today
                .end_utc(tz_offset_secs, day_start_minutes)
                .unwrap_or_else(|| now + FALLBACK_EXPIRY);
            tracing::info!(app = %record.key, "blocking app due to downtime schedule");
            db.set_block(subject, "downtime", now, Some(expires))?;
            return Ok(());
        }

        let snapshot = db.day_snapshot(today)?;
        let decision = engine.evaluate(
            record.id,
            &record.all_categories(),
            blockable,
            today.weekday_index().unwrap_or(0),
            &snapshot,
            now,
        );

        match decision {
            Decision::Block { binding } => {
                let expires = today
                    .end_utc(tz_offset_secs, day_start_minutes)
                    .unwrap_or_else(|| now + FALLBACK_EXPIRY);
                tracing::info!(app = %record.key, ?binding, expires = %expires, "blocking app");
                db.set_block(subject, "limit", now, Some(expires))?;
            }
            Decision::Allow { .. } | Decision::Warn { .. } => {
                db.clear_block(subject)?;
            }
        }
        Ok(())
    }

    /// Clear blocks whose stored deadline has passed.
    ///
    /// This replaces the old "wipe everything on rollover" as the primary thaw
    /// path: because every block now records when it dies, thawing needs no
    /// memory at all and works across restarts. Rows with no parsable expiry
    /// are treated as expired on purpose -- they can only come from a pre-expiry
    /// writer or tampering, and both should lift, not haunt the user forever.
    fn thaw_expired_blocks(&self, db: &mut Db, now: DateTime<Utc>) -> anyhow::Result<()> {
        let mut stmt = db
            .conn()
            .prepare("SELECT subject_type, subject_id, expires_utc FROM block_state")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;

        let mut expired: Vec<SubjectRef> = Vec::new();
        for row in rows {
            let (kind, id, expires_utc) = row?;
            let dead = match expires_utc.as_deref().map(DateTime::parse_from_rfc3339) {
                Some(Ok(expires)) => expires.with_timezone(&Utc) <= now,
                // Missing or malformed timestamp: fail safe by lifting it. See
                // the doc comment above for why silence would be worse.
                _ => true,
            };
            if dead {
                match subject_from_row(&kind, id) {
                    Some(subject) => expired.push(subject),
                    None => tracing::warn!(
                        subject_type = %kind,
                        "skipping block_state row with unknown subject type"
                    ),
                }
            }
        }

        for subject in expired {
            db.clear_block(subject)?;
            tracing::info!(subject = ?subject, "block expired; thawing");
        }
        Ok(())
    }

    /// Clear every remaining block. Only called on a day change this process
    /// observed itself, where expiry alone might leave a block alive across
    /// the boundary (e.g. a clock jump that lands us in tomorrow).
    fn clear_all_blocks(&self, db: &mut Db) -> anyhow::Result<()> {
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

/// Decode a `(subject_type, subject_id)` column pair.
///
/// Mirrors the private codec in `st-storage`; the agent only ever reads rows
/// that storage wrote, but storage does not export the inverse, so the two
/// encodings must be kept in step by hand. Unknown kinds are skipped with a
/// warning, same as storage does.
fn subject_from_row(kind: &str, id: i64) -> Option<SubjectRef> {
    match kind {
        "app" => Some(SubjectRef::App(id)),
        "site" => Some(SubjectRef::Site(id)),
        _ => None,
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
    use rusqlite::params;
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
        target: st_core::limits::LimitTarget,
        now: DateTime<Utc>,
    ) {
        let engine = engine_for(target);
        enforcer
            .tick(db, &engine, Some(key), None, now, 0, 0)
            .expect("tick");
    }

    #[test]
    fn exhausted_limit_records_a_block() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");

        let app = seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");
        snapshot_usage(&mut db, app, 60 * 60);

        let mut enforcer = Enforcer::new();
        tick_blocking(
            &mut enforcer,
            &mut db,
            &steam_key(),
            st_core::limits::LimitTarget::Category(games),
            at("2026-08-20T12:00:00Z"),
        );

        assert!(db.is_blocked(SubjectRef::App(app)).expect("blocked"));
    }

    #[test]
    fn downtime_schedule_blocks_non_allowlisted_apps() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");

        let app = seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");
        snapshot_usage(&mut db, app, 10 * 60);

        let mut enforcer = Enforcer::new();
        tick_blocking(
            &mut enforcer,
            &mut db,
            &steam_key(),
            st_core::limits::LimitTarget::Category(games),
            at("2026-08-20T12:00:00Z"),
        );
        assert!(db.is_blocked(SubjectRef::App(app)).expect("blocked"));

        // A +15 override lifts the budget -> next tick must clear the block.
        db.grant_override(
            &st_core::limits::LimitTarget::Category(games),
            DayKey(20260820),
            15 * 60,
            at("2026-08-20T12:01:00Z"),
            Some("test"),
        )
        .expect("override");
        tick_blocking(
            &mut enforcer,
            &mut db,
            &steam_key(),
            st_core::limits::LimitTarget::Category(games),
            at("2026-08-20T12:02:00Z"),
        );
        assert!(!db.is_blocked(SubjectRef::App(app)).expect("unblocked"));
    }

    #[test]
    fn never_block_apps_are_never_blocked() {
        let mut db = Db::open_in_memory().expect("db");

        let app = seed_app(&mut db, "C:\\code.exe", "development");
        snapshot_usage(&mut db, app, 99 * 60);

        let mut enforcer = Enforcer::new();
        // A total budget of zero would block any blockable app.
        tick_blocking(
            &mut enforcer,
            &mut db,
            &AppKey::windows_exe("C:\\code.exe"),
            st_core::limits::LimitTarget::Total,
            at("2026-08-20T12:00:00Z"),
        );

        assert!(!db.is_blocked(SubjectRef::App(app)).expect("not blocked"));
    }

    #[test]
    fn blocks_are_cleared_on_day_rollover() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");

        let app = seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");
        snapshot_usage(&mut db, app, 60 * 60);

        let mut enforcer = Enforcer::new();
        tick_blocking(
            &mut enforcer,
            &mut db,
            &steam_key(),
            st_core::limits::LimitTarget::Category(games),
            at("2026-08-20T22:00:00Z"),
        );
        assert!(db.is_blocked(SubjectRef::App(app)).expect("blocked"));

        // Next day: everything is unblocked.
        let engine = engine_for(st_core::limits::LimitTarget::Category(games));
        enforcer
            .tick(
                &mut db,
                &engine,
                None,
                None,
                at("2026-08-21T06:00:00Z"),
                0,
                0,
            )
            .expect("tick");
        assert!(!db.is_blocked(SubjectRef::App(app)).expect("cleared"));
    }

    #[test]
    fn a_zero_limit_blocks_immediately_without_usage() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");

        seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");
        let mut enforcer = Enforcer::new();
        tick_blocking(
            &mut enforcer,
            &mut db,
            &steam_key(),
            st_core::limits::LimitTarget::Category(games),
            at("2026-08-20T12:00:00Z"),
        );

        let blocked = db.blocked_subjects().expect("list");
        assert_eq!(blocked.len(), 1);
    }

    // -- Restart survival (the fail-open regression suite). ------------------

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .expect("valid rfc3339")
            .with_timezone(&Utc)
    }

    /// The core bug-1 regression: dropping the enforcer object (agent restart)
    /// must not lift a block whose day has not ended.
    #[test]
    fn a_live_block_survives_an_enforcer_restart_on_the_same_day() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        let app = seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");
        snapshot_usage(&mut db, app, 60 * 60);

        let mut enforcer = Enforcer::new();
        tick_blocking(
            &mut enforcer,
            &mut db,
            &steam_key(),
            st_core::limits::LimitTarget::Category(games),
            at("2026-08-20T12:00:00Z"),
        );
        // Discard the first enforcer: everything below is a fresh instance over
        // the same database — the "restart" under test.
        let _ = enforcer;
        let mut restarted = Enforcer::new();
        restarted
            .tick(
                &mut db,
                &engine_for(st_core::limits::LimitTarget::Category(games)),
                None,
                None,
                at("2026-08-20T12:05:00Z"),
                0,
                0,
            )
            .expect("first tick after restart");
        assert!(
            db.is_blocked(SubjectRef::App(app)).expect("check"),
            "restarting the agent must not lift a live block"
        );
    }

    /// A block whose persisted deadline passed is thawed by the next tick even
    /// with no day change and no prior state -- pure expiry semantics.
    #[test]
    fn an_expired_block_thaws_without_a_day_rollover() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        let app = seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");

        // Hand-set a short block so expiry falls inside one day.
        db.set_block(
            SubjectRef::App(app),
            "limit",
            at("2026-08-20T12:00:00Z"),
            Some(at("2026-08-20T12:01:00Z")),
        )
        .expect("set");

        let mut enforcer = Enforcer::new();
        enforcer
            .tick(
                &mut db,
                &engine_for(st_core::limits::LimitTarget::Category(games)),
                None,
                None,
                at("2026-08-20T12:02:00Z"),
                0,
                0,
            )
            .expect("tick");
        assert!(!db.is_blocked(SubjectRef::App(app)).expect("thawed"));
    }

    /// Rows written before blocks carried expiries (or with a corrupt stamp)
    /// must lift on the first tick, not survive forever.
    #[test]
    fn a_legacy_block_without_an_expiry_does_not_survive_startup() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        let app = seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");
        db.set_block(
            SubjectRef::App(app),
            "limit",
            at("2026-08-19T09:00:00Z"),
            None,
        )
        .expect("legacy set");

        let mut enforcer = Enforcer::new();
        enforcer
            .tick(
                &mut db,
                &engine_for(st_core::limits::LimitTarget::Category(games)),
                None,
                None,
                at("2026-08-20T12:00:00Z"),
                0,
                0,
            )
            .expect("tick");
        assert!(!db.is_blocked(SubjectRef::App(app)).expect("lifted"));
    }

    /// The persisted expiry must be the end of the local day, computed with
    /// the caller's timezone/day-start -- the whole reason `DayKey::end_utc`
    /// exists.
    #[test]
    fn a_new_block_expires_at_the_end_of_the_local_day() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        let app = seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");
        snapshot_usage(&mut db, app, 60 * 60);

        // 23:30 UTC in UTC+2 is already 01:30 local on the *21st*, so both the
        // attribution day and its end differ from the naive UTC-day answer.
        let tz = 2 * 3600;
        let mut enforcer = Enforcer::new();
        enforcer
            .tick(
                &mut db,
                &engine_for(st_core::limits::LimitTarget::Category(games)),
                Some(&steam_key()),
                None,
                at("2026-08-20T23:30:00Z"),
                tz,
                0,
            )
            .expect("tick");
        assert!(db.is_blocked(SubjectRef::App(app)).expect("blocked"));

        let end = DayKey(20260821).end_utc(tz, 0).expect("boundary");
        assert_eq!(end, at("2026-08-21T22:00:00Z"));

        let stored: Option<String> = db
            .conn()
            .query_row(
                "SELECT expires_utc FROM block_state WHERE subject_id = ?1",
                params![app],
                |row| row.get(0),
            )
            .expect("expiry column");
        assert_eq!(
            stored.as_deref().map(|s| DateTime::parse_from_rfc3339(s)
                .expect("rfc3339")
                .with_timezone(&Utc)),
            Some(end)
        );
    }
}
