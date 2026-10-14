//! Enforcement: translate the limits engine's verdicts into block state.
//!
//! On every evaluation tick the enforcer:
//!
//! 1. Lifts blocks whose `expires_utc` has passed.
//! 2. On a *genuine* day change observed across two ticks, clears whatever
//!    survived (every budget resets at the boundary).
//! 3. For each watched account with a fresh focus report, looks up the
//!    focused app's record (category + tags). Accounts nobody watches are
//!    never blocked (docs/adr/0005-per-account-usage.md).
//! 4. Asks the limits engine what to do (`Allow`/`Warn`/`Block`), against
//!    that account's own usage.
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
//! The agent never touches a blocked app's processes. Blocking is the block
//! screen: the session helper has the UI cover the app with an always-on-top
//! window (re-shown every 2 s while the block holds), which works for
//! anti-cheat-protected and elevated apps too. The app keeps running behind
//! it. `block_state` is the sole source of truth the session polls.

use chrono::{DateTime, Duration, Utc};
use st_core::category::CategoryKind;
use st_core::daykey::DayKey;
use st_core::limits::{Decision, LimitEngine};
use st_core::model::{AccountId, AppKey, SubjectRef};
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

    /// Evaluate and act on each account's focused app.
    ///
    /// `focused` holds every account with a fresh focus report and its focused
    /// app. Empty means nothing is evaluated, but expired blocks are still
    /// lifted and day rollover still happens.
    ///
    /// `now`, `tz_offset_secs` and `day_start_minutes` come from the caller's
    /// injected [`Clock`](st_core::clock::Clock) so tamper semantics stay
    /// testable, and match exactly how usage was bucketed into `today`.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        db: &mut Db,
        engine: &LimitEngine,
        focused: &[(AccountId, AppKey)],
        now: DateTime<Utc>,
        tz_offset_secs: i32,
        day_start_minutes: i64,
    ) -> anyhow::Result<()> {
        let today = DayKey::from_utc(now, tz_offset_secs, day_start_minutes);

        self.lift_expired_blocks(db, now)?;

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

        for (account, key) in focused {
            self.evaluate(
                db,
                engine,
                *account,
                key,
                now,
                today,
                tz_offset_secs,
                day_start_minutes,
            )?;
        }
        Ok(())
    }

    /// Act on one account's focused app.
    #[allow(clippy::too_many_arguments)]
    fn evaluate(
        &self,
        db: &mut Db,
        engine: &LimitEngine,
        account: AccountId,
        window_key: &AppKey,
        now: DateTime<Utc>,
        today: DayKey,
        tz_offset_secs: i32,
        day_start_minutes: i64,
    ) -> anyhow::Result<()> {
        if !db.is_supervised(account)? {
            // Limits don't apply here; make sure nothing lingers either.
            db.clear_account_blocks(account)?;
            return Ok(());
        }

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

        // 1. Check Allowlist: if allowlisted, never block via downtime
        let allowlisted = db.is_subject_allowlisted("app", record.id)?;

        // 2. Check Downtime Schedules
        // Wall-clock minute and weekday from the same local instant; see
        // `local_minute_and_weekday` for why `today` must not be used here.
        let (minute_of_day, weekday_idx) =
            st_core::schedules::local_minute_and_weekday(now, tz_offset_secs);
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
            st_core::schedules::is_schedule_active(&sched, minute_of_day, weekday_idx)
        });

        if blockable && downtime_active && !allowlisted {
            let expires = today
                .end_utc(tz_offset_secs, day_start_minutes)
                .unwrap_or_else(|| now + FALLBACK_EXPIRY);
            tracing::info!(app = %record.key, %account, "blocking app due to downtime schedule");
            db.set_block(account, subject, "downtime", now, Some(expires))?;
            return Ok(());
        }

        let snapshot = db.day_snapshot(account, today)?;
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
                tracing::info!(app = %record.key, %account, ?binding, expires = %expires, "blocking app");
                db.set_block(account, subject, "limit", now, Some(expires))?;
            }
            Decision::Allow { .. } | Decision::Warn { .. } => {
                db.clear_block(account, subject)?;
            }
        }
        Ok(())
    }

    /// Clear blocks whose stored deadline has passed.
    ///
    /// This replaces the old "wipe everything on rollover" as the primary way
    /// blocks end: because every block records when it dies, lifting needs no
    /// memory at all and works across restarts. Rows with no parsable expiry
    /// are treated as expired on purpose -- they can only come from a pre-expiry
    /// writer or tampering, and both should lift, not haunt the user forever.
    fn lift_expired_blocks(&self, db: &mut Db, now: DateTime<Utc>) -> anyhow::Result<()> {
        for block in db.all_blocks()? {
            let dead = match block
                .expires_utc
                .as_deref()
                .map(DateTime::parse_from_rfc3339)
            {
                Some(Ok(expires)) => expires.with_timezone(&Utc) <= now,
                // Missing or malformed timestamp: fail safe by lifting it. See
                // the doc comment above for why silence would be worse.
                _ => true,
            };
            if dead {
                db.clear_block(block.account, block.subject)?;
                tracing::info!(subject = ?block.subject, account = %block.account, "block expired; lifting it");
            }
        }
        Ok(())
    }

    /// Clear every remaining block. Only called on a day change this process
    /// observed itself, where expiry alone might leave a block alive across
    /// the boundary (e.g. a clock jump that lands us in tomorrow).
    fn clear_all_blocks(&self, db: &mut Db) -> anyhow::Result<()> {
        for block in db.all_blocks()? {
            let _ = db.clear_block(block.account, block.subject);
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

    const ACCT: AccountId = AccountId(1);
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
            account: ACCT,
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
            .tick(db, &engine, &[(ACCT, key.clone())], now, 0, 0)
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

        assert!(db.is_blocked(ACCT, SubjectRef::App(app)).expect("blocked"));
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
        assert!(db.is_blocked(ACCT, SubjectRef::App(app)).expect("blocked"));

        // A +15 override lifts the budget -> next tick must clear the block.
        db.grant_override(
            ACCT,
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
        assert!(!db
            .is_blocked(ACCT, SubjectRef::App(app))
            .expect("unblocked"));
    }

    /// Regression: with a non-midnight day start, the overnight window's
    /// morning half used to be checked against the wrong weekday bit.
    #[test]
    fn overnight_downtime_blocks_in_the_small_hours_with_a_late_day_start() {
        let mut db = Db::open_in_memory().expect("db");
        let app = seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");
        // Monday-only Bedtime, 22:00-07:00.
        let id = db
            .create_schedule("Bedtime", 0b000_0001, 22 * 60, 7 * 60)
            .expect("schedule");
        assert!(id > 0);

        let mut enforcer = Enforcer::new();
        let no_limits = LimitEngine::with_default_warnings(vec![]);
        // 2026-08-18 is a Tuesday: 02:00 local belongs to Monday's window,
        // even though a 04:00 day start still files it under Monday's DayKey.
        enforcer
            .tick(
                &mut db,
                &no_limits,
                &[(ACCT, steam_key())],
                at("2026-08-18T02:00:00Z"),
                0,
                4 * 60,
            )
            .expect("tick");
        assert!(db.is_blocked(ACCT, SubjectRef::App(app)).expect("blocked"));

        // The allowlist exempts an app from downtime.
        db.set_allowlist_subject("app", app, true).expect("allow");
        db.clear_block(ACCT, SubjectRef::App(app)).expect("clear");
        enforcer
            .tick(
                &mut db,
                &no_limits,
                &[(ACCT, steam_key())],
                at("2026-08-18T02:01:00Z"),
                0,
                4 * 60,
            )
            .expect("tick");
        assert!(!db
            .is_blocked(ACCT, SubjectRef::App(app))
            .expect("not blocked"));
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

        assert!(!db
            .is_blocked(ACCT, SubjectRef::App(app))
            .expect("not blocked"));
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
        assert!(db.is_blocked(ACCT, SubjectRef::App(app)).expect("blocked"));

        // Next day: everything is unblocked.
        let engine = engine_for(st_core::limits::LimitTarget::Category(games));
        enforcer
            .tick(&mut db, &engine, &[], at("2026-08-21T06:00:00Z"), 0, 0)
            .expect("tick");
        assert!(!db.is_blocked(ACCT, SubjectRef::App(app)).expect("cleared"));
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

        let blocked = db.blocked_subjects(ACCT).expect("list");
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
                &[],
                at("2026-08-20T12:05:00Z"),
                0,
                0,
            )
            .expect("first tick after restart");
        assert!(
            db.is_blocked(ACCT, SubjectRef::App(app)).expect("check"),
            "restarting the agent must not lift a live block"
        );
    }

    /// A block whose persisted deadline passed is lifted by the next tick even
    /// with no day change and no prior state -- pure expiry semantics.
    #[test]
    fn an_expired_block_lifts_without_a_day_rollover() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        let app = seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");

        // Hand-set a short block so expiry falls inside one day.
        db.set_block(
            ACCT,
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
                &[],
                at("2026-08-20T12:02:00Z"),
                0,
                0,
            )
            .expect("tick");
        assert!(!db.is_blocked(ACCT, SubjectRef::App(app)).expect("lifted"));
    }

    /// Rows written before blocks carried expiries (or with a corrupt stamp)
    /// must lift on the first tick, not survive forever.
    #[test]
    fn a_legacy_block_without_an_expiry_does_not_survive_startup() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        let app = seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");
        db.set_block(
            ACCT,
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
                &[],
                at("2026-08-20T12:00:00Z"),
                0,
                0,
            )
            .expect("tick");
        assert!(!db.is_blocked(ACCT, SubjectRef::App(app)).expect("lifted"));
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
                &[(ACCT, steam_key())],
                at("2026-08-20T23:30:00Z"),
                tz,
                0,
            )
            .expect("tick");
        assert!(db.is_blocked(ACCT, SubjectRef::App(app)).expect("blocked"));

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

    /// Limits apply per account: a watched account's own usage blocks it,
    /// and an account nobody watches is never blocked.
    #[test]
    fn each_account_is_judged_on_its_own_usage_and_unwatched_ones_never_block() {
        let mut db = Db::open_in_memory().expect("db");
        let sam = db
            .ensure_account("S-1-5-21-1", "Sam", at("2026-08-20T09:00:00Z"))
            .expect("sam");
        let alex = db
            .ensure_account("S-1-5-21-2", "Alex", at("2026-08-20T09:00:00Z"))
            .expect("alex");
        let kim = db
            .ensure_account("S-1-5-21-3", "Kim", at("2026-08-20T09:00:00Z"))
            .expect("kim");
        assert_eq!(sam, ACCT, "the fixtures record usage for the first account");
        db.set_account_supervised(kim, false).expect("unwatch kim");
        let games = db.category_id("games").expect("games");
        let app = seed_app(&mut db, "C:\\games\\steam\\steam.exe", "games");
        snapshot_usage(&mut db, app, 60 * 60);
        db.record_interval(&st_core::model::UsageInterval {
            subject: SubjectRef::App(app),
            account: kim,
            start: chrono::Utc::now(),
            end: chrono::Utc::now() + chrono::Duration::seconds(3600),
            day_key: DayKey(20260820),
        })
        .expect("kim's usage");
        // A block left over from before Kim stopped being watched.
        db.set_block(
            kim,
            SubjectRef::App(app),
            "limit",
            at("2026-08-20T11:00:00Z"),
            None,
        )
        .expect("stale block");

        let mut enforcer = Enforcer::new();
        enforcer
            .tick(
                &mut db,
                // 30 minutes: Sam's hour uses it up, Alex's nothing does not.
                &LimitEngine::with_default_warnings(vec![Limit::new(
                    1,
                    st_core::limits::LimitTarget::Category(games),
                    30,
                )]),
                &[(sam, steam_key()), (alex, steam_key()), (kim, steam_key())],
                at("2026-08-20T12:00:00Z"),
                0,
                0,
            )
            .expect("tick");

        assert!(
            db.is_blocked(sam, SubjectRef::App(app)).expect("sam"),
            "used it up"
        );
        assert!(
            !db.is_blocked(alex, SubjectRef::App(app)).expect("alex"),
            "no usage of their own"
        );
        assert!(
            !db.is_blocked(kim, SubjectRef::App(app)).expect("kim"),
            "not watched"
        );
    }
}
