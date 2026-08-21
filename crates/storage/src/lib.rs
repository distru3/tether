//! SQLite persistence for the screentime agent.
//!
//! The database lives in a directory owned by SYSTEM/root with restrictive
//! ACLs. The UI never opens it directly; it asks the agent over IPC. That is
//! what stops "just edit the database" from being the easiest bypass.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use st_core::category::{CategoryKind, BUILTIN_CATEGORIES};
use st_core::daykey::DayKey;
use st_core::limits::{Limit, LimitTarget, UsageSnapshot};
use st_core::model::{AppKey, CategoryId, SubjectRef, UsageInterval};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("malformed json in column {column}: {source}")]
    Json {
        column: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("unknown category slug: {0}")]
    UnknownCategory(String),
}

/// Ordered migrations. Append only; never edit a shipped migration.
const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../migrations/0001_init.sql"))];

pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open (creating if needed) and bring the schema up to date.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::bootstrap(conn)
    }

    /// In-memory database, for tests.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::bootstrap(conn)
    }

    fn bootstrap(conn: Connection) -> Result<Self> {
        // WAL keeps the 1 Hz sampler from blocking dashboard reads.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        let mut db = Self { conn };
        db.migrate()?;
        db.seed_builtin_categories()?;
        Ok(db)
    }

    fn migrate(&mut self) -> Result<()> {
        let current: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;

        for (version, sql) in MIGRATIONS {
            if *version <= current {
                continue;
            }
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", *version)?;
            tx.commit()?;
            tracing::info!(version, "applied migration");
        }
        Ok(())
    }

    /// Insert any built-in category that is missing.
    ///
    /// Idempotent, and runs on every start so that categories added in a later
    /// release appear without a migration.
    pub fn seed_builtin_categories(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO categories (slug, name, kind, color, builtin)
                 VALUES (?1, ?2, ?3, ?4, 1)
                 ON CONFLICT(slug) DO UPDATE SET
                     kind = excluded.kind,
                     color = excluded.color",
            )?;
            for c in BUILTIN_CATEGORIES {
                stmt.execute(params![c.slug, c.name, kind_to_str(c.kind), c.color])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn category_id(&self, slug: &str) -> Result<CategoryId> {
        self.conn
            .query_row(
                "SELECT id FROM categories WHERE slug = ?1",
                params![slug],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| StorageError::UnknownCategory(slug.to_string()))
    }

    /// Record that an app was seen, returning its id.
    ///
    /// Never touches `primary_category_id` on an existing row: re-classifying
    /// silently would throw away the user's decision and their limits with it.
    pub fn upsert_app(
        &self,
        key: &AppKey,
        display_name: &str,
        publisher: Option<&str>,
        default_category: CategoryId,
        now: DateTime<Utc>,
    ) -> Result<i64> {
        let now_s = now.to_rfc3339();
        let id = self.conn.query_row(
            "INSERT INTO apps
                 (app_key, display_name, publisher, primary_category_id,
                  first_seen_utc, last_seen_utc)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(app_key) DO UPDATE SET
                 last_seen_utc = excluded.last_seen_utc,
                 display_name  = excluded.display_name
             RETURNING id",
            params![
                key.to_db_string(),
                display_name,
                publisher,
                default_category,
                now_s
            ],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    pub fn set_app_categories(
        &mut self,
        app_id: i64,
        primary: CategoryId,
        tags: &[CategoryId],
        by_user: bool,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE apps SET primary_category_id = ?2, user_classified = ?3 WHERE id = ?1",
            params![app_id, primary, by_user as i64],
        )?;
        tx.execute("DELETE FROM app_tags WHERE app_id = ?1", params![app_id])?;
        {
            let mut stmt =
                tx.prepare("INSERT OR IGNORE INTO app_tags (app_id, category_id) VALUES (?1, ?2)")?;
            for tag in tags {
                if *tag != primary {
                    stmt.execute(params![app_id, tag])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Persist a usage interval and fold it into the daily rollup atomically.
    pub fn record_interval(&mut self, interval: &UsageInterval) -> Result<()> {
        let (subject_type, subject_id) = subject_to_row(interval.subject);
        let secs = interval.duration_secs();

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO usage_intervals
                 (subject_type, subject_id, session_id, start_utc, end_utc,
                  duration_secs, day_key)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                subject_type,
                subject_id,
                interval.session_id,
                interval.start.to_rfc3339(),
                interval.end.to_rfc3339(),
                secs,
                interval.day_key.0
            ],
        )?;
        tx.execute(
            "INSERT INTO usage_daily (day_key, subject_type, subject_id, seconds)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(day_key, subject_type, subject_id)
             DO UPDATE SET seconds = seconds + excluded.seconds",
            params![interval.day_key.0, subject_type, subject_id, secs],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Rebuild the rollup for one day from raw intervals.
    ///
    /// Used by the repair command and after importing data; the rollup is
    /// otherwise maintained incrementally.
    pub fn rollup_day(&mut self, day: DayKey) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM usage_daily WHERE day_key = ?1", params![day.0])?;
        tx.execute(
            "INSERT INTO usage_daily (day_key, subject_type, subject_id, seconds)
             SELECT day_key, subject_type, subject_id, SUM(duration_secs)
             FROM usage_intervals
             WHERE day_key = ?1
             GROUP BY day_key, subject_type, subject_id",
            params![day.0],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn load_limits(&self) -> Result<Vec<Limit>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, target_type, target_id, default_minutes, weekday_minutes, enabled
             FROM limits",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, target_type, target_id, default_minutes, weekday_json, enabled) = row?;
            let Some(target) = row_to_target(&target_type, target_id) else {
                tracing::warn!(target_type, "skipping limit with unknown target type");
                continue;
            };
            let parsed: Vec<Option<u32>> =
                serde_json::from_str(&weekday_json).map_err(|source| StorageError::Json {
                    column: "limits.weekday_minutes",
                    source,
                })?;
            let mut weekday_minutes = [None; 7];
            for (slot, value) in weekday_minutes.iter_mut().zip(parsed) {
                *slot = value;
            }
            out.push(Limit {
                id,
                target,
                default_minutes: default_minutes.max(0) as u32,
                weekday_minutes,
                enabled: enabled != 0,
            });
        }
        Ok(out)
    }

    pub fn upsert_limit(&self, limit: &Limit, now: DateTime<Utc>) -> Result<()> {
        let (target_type, target_id) = target_to_row(&limit.target);
        let weekday_json = serde_json::to_string(&limit.weekday_minutes.to_vec())
            .expect("array of options always serialises");
        self.conn.execute(
            "INSERT INTO limits
                 (target_type, target_id, default_minutes, weekday_minutes, enabled, created_utc)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(target_type, target_id) DO UPDATE SET
                 default_minutes = excluded.default_minutes,
                 weekday_minutes = excluded.weekday_minutes,
                 enabled         = excluded.enabled",
            params![
                target_type,
                target_id,
                limit.default_minutes,
                weekday_json,
                limit.enabled as i64,
                now.to_rfc3339()
            ],
        )?;
        Ok(())
    }

    /// Everything the limits engine needs for one day, in three queries.
    pub fn day_snapshot(&self, day: DayKey) -> Result<DaySnapshot> {
        let mut used: HashMap<LimitTarget, i64> = HashMap::new();

        // Per app.
        let mut stmt = self.conn.prepare(
            "SELECT subject_id, seconds FROM usage_daily
             WHERE day_key = ?1 AND subject_type = 'app'",
        )?;
        for row in stmt.query_map(params![day.0], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })? {
            let (app_id, secs) = row?;
            used.insert(LimitTarget::App(app_id), secs);
        }

        // Per category, through the primary-plus-tags view so that a tagged app
        // counts toward every category that limits it.
        let mut stmt = self.conn.prepare(
            "SELECT ac.category_id, COALESCE(SUM(ud.seconds), 0)
             FROM usage_daily ud
             JOIN app_categories ac ON ac.app_id = ud.subject_id
             WHERE ud.subject_type = 'app' AND ud.day_key = ?1
             GROUP BY ac.category_id",
        )?;
        for row in stmt.query_map(params![day.0], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })? {
            let (category_id, secs) = row?;
            used.insert(LimitTarget::Category(category_id), secs);
        }

        // Total, excluding never-blockable apps so that time in a terminal does
        // not eat the budget for everything else.
        let total: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(ud.seconds), 0)
             FROM usage_daily ud
             JOIN apps a ON a.id = ud.subject_id
             JOIN categories c ON c.id = a.primary_category_id
             WHERE ud.subject_type = 'app'
               AND ud.day_key = ?1
               AND c.kind != 'never_block'",
            params![day.0],
            |row| row.get(0),
        )?;
        used.insert(LimitTarget::Total, total);

        // Active overrides.
        let mut granted: HashMap<LimitTarget, i64> = HashMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT target_type, target_id, SUM(granted_secs)
             FROM overrides WHERE day_key = ?1
             GROUP BY target_type, target_id",
        )?;
        for row in stmt.query_map(params![day.0], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })? {
            let (target_type, target_id, secs) = row?;
            if let Some(target) = row_to_target(&target_type, target_id) {
                granted.insert(target, secs);
            }
        }

        Ok(DaySnapshot { day, used, granted })
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn setting_i64(&self, key: &str, fallback: i64) -> i64 {
        self.setting(key)
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(fallback)
    }

    /// Append to the append-only audit log.
    pub fn audit(&self, now: DateTime<Utc>, kind: &str, detail: Option<&str>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO audit_log (ts_utc, kind, detail) VALUES (?1, ?2, ?3)",
            params![now.to_rfc3339(), kind, detail],
        )?;
        Ok(())
    }

    /// Escape hatch for queries not yet wrapped in a method.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

/// One day's usage, ready to feed the limits engine.
pub struct DaySnapshot {
    pub day: DayKey,
    used: HashMap<LimitTarget, i64>,
    granted: HashMap<LimitTarget, i64>,
}

impl UsageSnapshot for DaySnapshot {
    fn seconds_used(&self, target: &LimitTarget) -> i64 {
        self.used.get(target).copied().unwrap_or(0)
    }

    fn granted_extra_secs(&self, target: &LimitTarget) -> i64 {
        self.granted.get(target).copied().unwrap_or(0)
    }
}

fn kind_to_str(kind: CategoryKind) -> &'static str {
    match kind {
        CategoryKind::Limitable => "limitable",
        CategoryKind::BlockOnly => "block_only",
        CategoryKind::NeverBlock => "never_block",
    }
}

fn subject_to_row(subject: SubjectRef) -> (&'static str, i64) {
    match subject {
        SubjectRef::App(id) => ("app", id),
        SubjectRef::Site(id) => ("site", id),
    }
}

fn target_to_row(target: &LimitTarget) -> (&'static str, Option<i64>) {
    match target {
        LimitTarget::App(id) => ("app", Some(*id)),
        LimitTarget::Category(id) => ("category", Some(*id)),
        LimitTarget::Total => ("total", None),
    }
}

fn row_to_target(kind: &str, id: Option<i64>) -> Option<LimitTarget> {
    match (kind, id) {
        ("app", Some(id)) => Some(LimitTarget::App(id)),
        ("category", Some(id)) => Some(LimitTarget::Category(id)),
        ("total", _) => Some(LimitTarget::Total),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use st_core::limits::LimitEngine;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-20T12:00:00Z")
            .expect("valid rfc3339")
            .with_timezone(&Utc)
    }

    fn interval(app_id: i64, secs: i64, day: DayKey) -> UsageInterval {
        UsageInterval {
            subject: SubjectRef::App(app_id),
            session_id: "s1".into(),
            start: now(),
            end: now() + chrono::Duration::seconds(secs),
            day_key: day,
        }
    }

    #[test]
    fn migrations_and_seed_are_idempotent() {
        let mut db = Db::open_in_memory().expect("open");
        db.seed_builtin_categories().expect("reseed");
        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count as usize, BUILTIN_CATEGORIES.len());
    }

    #[test]
    fn upsert_app_preserves_a_user_category() {
        let mut db = Db::open_in_memory().expect("open");
        let games = db.category_id("games").expect("games");
        let uncat = db.category_id("uncategorized").expect("uncategorized");

        let key = AppKey::windows_exe("C:\\Games\\Steam\\steam.exe");
        let id = db
            .upsert_app(&key, "Steam", None, uncat, now())
            .expect("insert");
        db.set_app_categories(id, games, &[], true)
            .expect("classify");

        // Seeing the app again must not reset the category.
        let same = db
            .upsert_app(&key, "Steam", None, uncat, now())
            .expect("upsert");
        assert_eq!(id, same);

        let (cat, user): (i64, i64) = db
            .conn()
            .query_row(
                "SELECT primary_category_id, user_classified FROM apps WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("read back");
        assert_eq!(cat, games);
        assert_eq!(user, 1);
    }

    #[test]
    fn app_key_normalisation_prevents_duplicate_rows() {
        let db = Db::open_in_memory().expect("open");
        let uncat = db.category_id("uncategorized").expect("uncategorized");

        let a = db
            .upsert_app(
                &AppKey::windows_exe("C:/Apps/Foo.exe"),
                "Foo",
                None,
                uncat,
                now(),
            )
            .expect("a");
        let b = db
            .upsert_app(
                &AppKey::windows_exe("c:\\apps\\foo.exe"),
                "Foo",
                None,
                uncat,
                now(),
            )
            .expect("b");
        assert_eq!(a, b, "case and separator differences must collapse");
    }

    #[test]
    fn usage_rolls_up_per_app_category_and_total() {
        let mut db = Db::open_in_memory().expect("open");
        let day = DayKey(20260820);
        let social = db.category_id("social-media").expect("social");
        let shortform = db.category_id("short-form-video").expect("shortform");
        let uncat = db.category_id("uncategorized").expect("uncat");

        let tiktok = db
            .upsert_app(
                &AppKey::windows_exe("C:\\tiktok.exe"),
                "TikTok",
                None,
                uncat,
                now(),
            )
            .expect("app");
        db.set_app_categories(tiktok, social, &[shortform], true)
            .expect("classify");

        db.record_interval(&interval(tiktok, 600, day)).expect("i1");
        db.record_interval(&interval(tiktok, 300, day)).expect("i2");

        let snap = db.day_snapshot(day).expect("snapshot");
        assert_eq!(snap.seconds_used(&LimitTarget::App(tiktok)), 900);
        // Counts against both categories it belongs to.
        assert_eq!(snap.seconds_used(&LimitTarget::Category(social)), 900);
        assert_eq!(snap.seconds_used(&LimitTarget::Category(shortform)), 900);
        assert_eq!(snap.seconds_used(&LimitTarget::Total), 900);
    }

    #[test]
    fn never_block_apps_are_excluded_from_the_total() {
        let mut db = Db::open_in_memory().expect("open");
        let day = DayKey(20260820);
        let dev = db.category_id("development").expect("dev");
        let uncat = db.category_id("uncategorized").expect("uncat");

        let term = db
            .upsert_app(
                &AppKey::windows_exe("C:\\wt.exe"),
                "Terminal",
                None,
                uncat,
                now(),
            )
            .expect("app");
        db.set_app_categories(term, dev, &[], true)
            .expect("classify");
        db.record_interval(&interval(term, 3600, day)).expect("i1");

        let snap = db.day_snapshot(day).expect("snapshot");
        assert_eq!(snap.seconds_used(&LimitTarget::App(term)), 3600);
        assert_eq!(snap.seconds_used(&LimitTarget::Total), 0);
    }

    #[test]
    fn rollup_rebuild_matches_incremental_totals() {
        let mut db = Db::open_in_memory().expect("open");
        let day = DayKey(20260820);
        let uncat = db.category_id("uncategorized").expect("uncat");
        let app = db
            .upsert_app(&AppKey::windows_exe("C:\\a.exe"), "A", None, uncat, now())
            .expect("app");

        db.record_interval(&interval(app, 120, day)).expect("i1");
        db.record_interval(&interval(app, 240, day)).expect("i2");
        let before = db
            .day_snapshot(day)
            .expect("snap")
            .seconds_used(&LimitTarget::App(app));

        db.rollup_day(day).expect("rebuild");
        let after = db
            .day_snapshot(day)
            .expect("snap")
            .seconds_used(&LimitTarget::App(app));

        assert_eq!(before, 360);
        assert_eq!(after, before);
    }

    #[test]
    fn limits_round_trip_including_weekday_overrides() {
        let db = Db::open_in_memory().expect("open");
        let social = db.category_id("social-media").expect("social");

        let mut limit = Limit::new(0, LimitTarget::Category(social), 30);
        limit.weekday_minutes[6] = Some(120);
        db.upsert_limit(&limit, now()).expect("insert");

        let loaded = db.load_limits().expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].default_minutes, 30);
        assert_eq!(loaded[0].minutes_for_weekday(6), 120);
        assert_eq!(loaded[0].minutes_for_weekday(0), 30);
    }

    #[test]
    fn engine_blocks_using_a_real_snapshot() {
        let mut db = Db::open_in_memory().expect("open");
        let day = DayKey(20260820);
        let shortform = db.category_id("short-form-video").expect("shortform");
        let uncat = db.category_id("uncategorized").expect("uncat");

        let app = db
            .upsert_app(
                &AppKey::windows_exe("C:\\tiktok.exe"),
                "TikTok",
                None,
                uncat,
                now(),
            )
            .expect("app");
        db.set_app_categories(app, shortform, &[], true)
            .expect("classify");
        db.record_interval(&interval(app, 20 * 60, day))
            .expect("usage");
        db.upsert_limit(&Limit::new(0, LimitTarget::Category(shortform), 15), now())
            .expect("limit");

        let engine = LimitEngine::with_default_warnings(db.load_limits().expect("limits"));
        let snap = db.day_snapshot(day).expect("snapshot");
        let weekday = day.weekday_index().expect("weekday");

        assert!(engine
            .evaluate(app, &[shortform], true, weekday, &snap)
            .is_blocked());
    }

    #[test]
    fn defaults_are_present_and_privacy_preserving() {
        let db = Db::open_in_memory().expect("open");
        assert_eq!(
            db.setting("capture_window_titles").expect("get").as_deref(),
            Some("false")
        );
        assert_eq!(
            db.setting("telemetry_enabled").expect("get").as_deref(),
            Some("false")
        );
        assert_eq!(db.setting_i64("idle_threshold_secs", 0), 60);
    }
}
