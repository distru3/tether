//! SQLite persistence for the screentime agent.
//!
//! The database lives in a directory owned by SYSTEM/root with restrictive
//! ACLs. The UI never opens it directly; it asks the agent over IPC. That is
//! what stops "just edit the database" from being the easiest bypass.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use st_core::category::{Category, CategoryKind, BUILTIN_CATEGORIES};
use st_core::daykey::DayKey;
use st_core::limits::{Limit, LimitTarget, UsageSnapshot};
use st_core::model::{AppKey, AppRecord, CategoryId, SubjectRef, UsageInterval};
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
    #[error("app not found: {0}")]
    AppNotFound(i64),
}

/// Ordered migrations. Append only; never edit a shipped migration.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../migrations/0001_init.sql")),
    (2, include_str!("../migrations/0002_pending_limits.sql")),
];

/// One row of `pending_limits`: id, target_type, target_id, action,
/// default_minutes, weekday_minutes (JSON), enabled.
type PendingRow = (
    i64,
    String,
    Option<i64>,
    String,
    Option<i64>,
    Option<String>,
    Option<i64>,
);

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

    /// The `kind` of a category, used to decide whether a limit may be attached.
    pub fn category_kind(&self, id: i64) -> Result<Option<CategoryKind>> {
        let kind_s: Option<String> = self
            .conn
            .query_row(
                "SELECT kind FROM categories WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(kind_s.and_then(|s| kind_from_str(&s)))
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

    /// Current classification of an app: its primary category and whether a
    /// human chose it. Lets the classifier skip rows it already owns, so it
    /// never rewrites the same verdict on every interval.
    pub fn app_category_state(&self, app_id: i64) -> Result<(CategoryId, bool)> {
        self.conn
            .query_row(
                "SELECT primary_category_id, user_classified FROM apps WHERE id = ?1",
                params![app_id],
                |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0)),
            )
            .optional()?
            .ok_or_else(|| StorageError::AppNotFound(app_id))
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

    /// Delete a limit immediately. Deleting is a *loosening* action, so the
    /// caller is responsible for honouring the anti-impulse cooldown before
    /// calling this.
    pub fn delete_limit(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM limits WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Delete a limit by its target, used when the cooldown has already elapsed
    /// (so the delete can apply immediately without knowing the row's id).
    pub fn delete_limit_by_target(&self, target: &LimitTarget) -> Result<()> {
        let (target_type, target_id) = target_to_row(target);
        let sql = match target_id {
            Some(tid) => format!(
                "DELETE FROM limits WHERE target_type = '{target_type}' AND target_id = {tid}"
            ),
            None => format!(
                "DELETE FROM limits WHERE target_type = '{target_type}' AND target_id IS NULL"
            ),
        };
        self.conn.execute(&sql, [])?;
        Ok(())
    }

    /// Queue a loosened limit change that takes effect only at
    /// `effective_from_utc`. The current `limits` row is left untouched, so the
    /// old (tighter) value keeps being enforced until the cooldown elapses.
    pub fn queue_pending_update(
        &self,
        target: &LimitTarget,
        default_minutes: u32,
        weekday_minutes: [Option<u32>; 7],
        enabled: bool,
        effective_from_utc: DateTime<Utc>,
    ) -> Result<()> {
        let (target_type, target_id) = target_to_row(target);
        let weekday_json = serde_json::to_string(&weekday_minutes.to_vec())
            .expect("array of options always serialises");
        self.conn.execute(
            "INSERT INTO pending_limits
                 (target_type, target_id, action, default_minutes, weekday_minutes,
                  enabled, effective_from_utc)
             VALUES (?1, ?2, 'update', ?3, ?4, ?5, ?6)
             ON CONFLICT(target_type, target_id) DO UPDATE SET
                 action             = 'update',
                 default_minutes    = excluded.default_minutes,
                 weekday_minutes    = excluded.weekday_minutes,
                 enabled            = excluded.enabled,
                 effective_from_utc = excluded.effective_from_utc",
            params![
                target_type,
                target_id,
                default_minutes,
                weekday_json,
                enabled as i64,
                effective_from_utc.to_rfc3339()
            ],
        )?;
        Ok(())
    }

    /// Queue a limit removal that takes effect at `effective_from_utc`.
    pub fn queue_pending_delete(
        &self,
        target: &LimitTarget,
        effective_from_utc: DateTime<Utc>,
    ) -> Result<()> {
        let (target_type, target_id) = target_to_row(target);
        self.conn.execute(
            "INSERT INTO pending_limits
                 (target_type, target_id, action, effective_from_utc)
             VALUES (?1, ?2, 'delete', ?3)
             ON CONFLICT(target_type, target_id) DO UPDATE SET
                 action             = 'delete',
                 default_minutes    = NULL,
                 weekday_minutes    = NULL,
                 enabled            = NULL,
                 effective_from_utc = excluded.effective_from_utc",
            params![target_type, target_id, effective_from_utc.to_rfc3339()],
        )?;
        Ok(())
    }

    /// Move any pending change whose time has come into the live `limits` table.
    /// Called by the enforcement loop every tick so a loosened limit starts
    /// applying the instant its cooldown elapses.
    pub fn promote_pending_limits(&mut self, now: DateTime<Utc>) -> Result<()> {
        let rows: Vec<PendingRow> = self
            .conn
            .prepare(
                "SELECT id, target_type, target_id, action, default_minutes, weekday_minutes, enabled
                 FROM pending_limits WHERE effective_from_utc <= ?1",
            )?
            .query_map(params![now.to_rfc3339()], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        tracing::debug!(pending = rows.len(), "promoting pending limits");

        let tx = self.conn.transaction()?;
        for (id, target_type, target_id, action, default_minutes, weekday_json, enabled) in rows {
            let Some(target) = row_to_target(&target_type, target_id) else {
                tracing::warn!(
                    id,
                    target_type,
                    "skipping pending limit with unknown target"
                );
                tx.execute("DELETE FROM pending_limits WHERE id = ?1", params![id])?;
                continue;
            };
            let (tt, tid) = target_to_row(&target);
            let scope = |tt: &str, tid: Option<i64>| -> String {
                match tid {
                    Some(tid) => format!("target_type = '{tt}' AND target_id = {tid}"),
                    // SQL `= NULL` never matches; the total budget has a NULL
                    // target_id and must use `IS NULL` instead.
                    None => format!("target_type = '{tt}' AND target_id IS NULL"),
                }
            };
            match action.as_str() {
                "delete" => {
                    let sql = format!("DELETE FROM limits WHERE {}", scope(tt, tid));
                    tx.execute(&sql, [])?;
                }
                "update" => {
                    let weekday_json = weekday_json.unwrap_or_default();
                    let weekday: Vec<Option<u32>> =
                        serde_json::from_str(&weekday_json).map_err(|source| {
                            StorageError::Json {
                                column: "pending_limits.weekday_minutes",
                                source,
                            }
                        })?;
                    if weekday.len() != 7 {
                        tracing::warn!(id, "pending limit with malformed weekday array");
                    } else {
                        let sql = format!(
                            "UPDATE limits SET
                                 default_minutes = ?1, weekday_minutes = ?2, enabled = ?3
                             WHERE {}",
                            scope(tt, tid)
                        );
                        tx.execute(&sql, params![default_minutes, weekday_json, enabled])?;
                    }
                }
                _ => {
                    tracing::warn!(id, action, "skipping malformed pending limit");
                }
            }
            tx.execute("DELETE FROM pending_limits WHERE id = ?1", params![id])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// All apps known to the system, with their current classification.
    pub fn list_apps(&self) -> Result<Vec<AppRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.id, a.app_key, a.display_name, a.publisher,
                    a.primary_category_id, a.user_classified
             FROM apps a
             ORDER BY a.display_name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)? != 0,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, key_s, display_name, publisher, primary_category, user_classified) = row?;
            let Some(key) = AppKey::parse_db_string(&key_s) else {
                tracing::warn!(app_key = %key_s, "skipping app with unparsable key");
                continue;
            };
            let tags = self.app_tags(id)?;
            out.push(AppRecord {
                id,
                key,
                display_name,
                publisher,
                primary_category,
                tags,
                user_classified,
            });
        }
        Ok(out)
    }

    /// A single app record, for the enforcement loop and the categoriser.
    pub fn app_record(&self, app_id: i64) -> Result<Option<AppRecord>> {
        let row = self.conn.query_row(
            "SELECT a.id, a.app_key, a.display_name, a.publisher,
                    a.primary_category_id, a.user_classified
             FROM apps a WHERE a.id = ?1",
            params![app_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)? != 0,
                ))
            },
        );
        let Ok((id, key_s, display_name, publisher, primary_category, user_classified)) = row
        else {
            return Ok(None);
        };
        let Some(key) = AppKey::parse_db_string(&key_s) else {
            return Ok(None);
        };
        Ok(Some(AppRecord {
            id,
            key,
            display_name,
            publisher,
            primary_category,
            tags: self.app_tags(id)?,
            user_classified,
        }))
    }

    fn app_tags(&self, app_id: i64) -> Result<Vec<CategoryId>> {
        let mut stmt = self
            .conn
            .prepare("SELECT category_id FROM app_tags WHERE app_id = ?1 ORDER BY category_id")?;
        let rows = stmt.query_map(params![app_id], |row| row.get(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// All categories, so the UI can render the editor without hard-coding ids.
    pub fn list_categories(&self) -> Result<Vec<Category>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, slug, name, kind, color, builtin FROM categories ORDER BY id")?;
        let rows = stmt.query_map([], |row| {
            let kind_s: String = row.get(3)?;
            Ok(Category {
                id: row.get(0)?,
                slug: row.get(1)?,
                name: row.get(2)?,
                kind: kind_from_str(&kind_s)
                    .unwrap_or(CategoryKind::Limitable)
                    .to_owned(),
                color: row.get(4)?,
                builtin: row.get::<_, i64>(5)? != 0,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Dashboard-shaped limits with human-readable labels, for the editor.
    pub fn list_limit_rows(&self) -> Result<Vec<LimitRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT l.id, l.target_type, l.target_id, l.default_minutes,
                    l.weekday_minutes, l.enabled,
                    COALESCE(a.display_name, c.name, 'Total')
             FROM limits l
             LEFT JOIN apps a ON a.id = l.target_id AND l.target_type = 'app'
             LEFT JOIN categories c ON c.id = l.target_id AND l.target_type = 'category'
             ORDER BY l.id",
        )?;
        let rows = stmt.query_map([], |row| {
            let target_type: String = row.get(1)?;
            let target_id: Option<i64> = row.get(2)?;
            let target = match (target_type.as_str(), target_id) {
                ("app", Some(id)) => Some(LimitTarget::App(id)),
                ("category", Some(id)) => Some(LimitTarget::Category(id)),
                ("total", _) => Some(LimitTarget::Total),
                _ => None,
            };
            let weekday_json: String = row.get(4)?;
            Ok(LimitRow {
                id: row.get(0)?,
                target,
                default_minutes: row.get(3)?,
                weekday_json,
                weekday_minutes: [None; 7],
                enabled: row.get::<_, i64>(5)? != 0,
                label: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            let mut r = row?;
            if r.target.is_none() {
                tracing::warn!(id = r.id, "skipping limit with unknown target type");
                continue;
            }
            // Normalise weekday json into the same array shape as `load_limits`.
            let parsed: Vec<Option<u32>> =
                serde_json::from_str(&r.weekday_json).map_err(|source| StorageError::Json {
                    column: "limits.weekday_minutes",
                    source,
                })?;
            let mut weekday_minutes = [None; 7];
            for (slot, value) in weekday_minutes.iter_mut().zip(parsed) {
                *slot = value;
            }
            r.weekday_minutes = weekday_minutes;
            out.push(r);
        }
        Ok(out)
    }

    /// PIN hash, if one has been set. `None` means "no PIN configured", which
    /// in M2 lets limit edits happen without one (see the plan decision).
    pub fn pin_hash(&self) -> Result<Option<String>> {
        self.setting("pin_hash")
    }

    pub fn set_pin_hash(&self, hash: &str) -> Result<()> {
        self.set_setting("pin_hash", hash)
    }

    /// Record a PIN-approved extension, additive for the given day.
    pub fn grant_override(
        &self,
        target: &LimitTarget,
        day: DayKey,
        seconds: i64,
        now: DateTime<Utc>,
        reason: Option<&str>,
    ) -> Result<()> {
        let (target_type, target_id) = target_to_row(target);
        self.conn.execute(
            "INSERT INTO overrides (target_type, target_id, day_key, granted_secs, granted_utc, reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![target_type, target_id, day.0, seconds, now.to_rfc3339(), reason],
        )?;
        self.audit(now, "override_granted", reason)
    }

    /// Freeze an app until `expires_utc` (typically the day boundary). Replaces
    /// any existing block so a re-block updates the deadline.
    pub fn set_block(
        &self,
        subject: SubjectRef,
        reason: &str,
        now: DateTime<Utc>,
        expires_utc: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let (subject_type, subject_id) = subject_to_row(subject);
        self.conn.execute(
            "INSERT INTO block_state (subject_type, subject_id, reason, blocked_since_utc, expires_utc)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(subject_type, subject_id) DO UPDATE SET
                 reason            = excluded.reason,
                 blocked_since_utc = excluded.blocked_since_utc,
                 expires_utc       = excluded.expires_utc",
            params![
                subject_type,
                subject_id,
                reason,
                now.to_rfc3339(),
                expires_utc.map(|t| t.to_rfc3339())
            ],
        )?;
        Ok(())
    }

    pub fn clear_block(&self, subject: SubjectRef) -> Result<()> {
        let (subject_type, subject_id) = subject_to_row(subject);
        self.conn.execute(
            "DELETE FROM block_state WHERE subject_type = ?1 AND subject_id = ?2",
            params![subject_type, subject_id],
        )?;
        Ok(())
    }

    /// Which apps are currently blocked, for the dashboard's blocked flag and
    /// for day-rollover thawing.
    pub fn blocked_subjects(&self) -> Result<Vec<(SubjectRef, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT subject_type, subject_id, reason FROM block_state")?;
        let rows = stmt.query_map([], |row| {
            let st: String = row.get(0)?;
            let subject = match st.as_str() {
                "app" => SubjectRef::App(row.get(1)?),
                "site" => SubjectRef::Site(row.get(1)?),
                other => {
                    tracing::warn!(subject_type = other, "unknown subject type in block_state");
                    return Err(rusqlite::Error::InvalidQuery);
                }
            };
            Ok((subject, row.get::<_, String>(2)?))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Whether an app is currently blocked.
    pub fn is_blocked(&self, subject: SubjectRef) -> Result<bool> {
        let (subject_type, subject_id) = subject_to_row(subject);
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM block_state WHERE subject_type = ?1 AND subject_id = ?2",
            params![subject_type, subject_id],
            |row| row.get(0),
        )?;
        Ok(n > 0)
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

    /// Dashboard rows for one day: per-app and per-primary-category usage,
    /// already sorted descending. Tags are excluded from the category rows so
    /// they sum to `total_seconds` and the chart cannot exceed 100%.
    pub fn day_summary(&self, day: DayKey) -> Result<DaySummary> {
        let total_seconds: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(ud.seconds), 0)
             FROM usage_daily ud
             JOIN apps a ON a.id = ud.subject_id
             WHERE ud.subject_type = 'app' AND ud.day_key = ?1",
            params![day.0],
            |row| row.get(0),
        )?;

        let mut stmt = self.conn.prepare(
            "SELECT a.id, a.display_name, a.primary_category_id, c.name, c.color, ud.seconds
             FROM usage_daily ud
             JOIN apps a ON a.id = ud.subject_id
             JOIN categories c ON c.id = a.primary_category_id
             WHERE ud.subject_type = 'app' AND ud.day_key = ?1
             ORDER BY ud.seconds DESC, a.display_name",
        )?;
        let apps = stmt
            .query_map(params![day.0], |row| {
                Ok(UsageRow {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    category_id: row.get(2)?,
                    category_name: row.get(3)?,
                    category_color: row.get(4)?,
                    seconds: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.name, c.color, SUM(ud.seconds)
             FROM usage_daily ud
             JOIN apps a ON a.id = ud.subject_id
             JOIN categories c ON c.id = a.primary_category_id
             WHERE ud.subject_type = 'app' AND ud.day_key = ?1
             GROUP BY c.id, c.name, c.color
             ORDER BY SUM(ud.seconds) DESC, c.name",
        )?;
        let categories = stmt
            .query_map(params![day.0], |row| {
                Ok(CategoryRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    seconds: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(DaySummary {
            day,
            total_seconds,
            apps,
            categories,
        })
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

/// Dashboard-shaped usage for one day. Storage returns this; the agent maps it
/// to the IPC `DaySummaryDto` so the storage crate never depends on `st-ipc`.
pub struct DaySummary {
    pub day: DayKey,
    pub total_seconds: i64,
    pub apps: Vec<UsageRow>,
    pub categories: Vec<CategoryRow>,
}

pub struct UsageRow {
    pub id: i64,
    pub label: String,
    pub category_id: i64,
    pub category_name: String,
    pub category_color: String,
    pub seconds: i64,
}

pub struct CategoryRow {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub seconds: i64,
}

/// A limit plus its human-readable label, for the editor UI.
pub struct LimitRow {
    pub id: i64,
    pub target: Option<LimitTarget>,
    pub default_minutes: i64,
    pub weekday_minutes: [Option<u32>; 7],
    /// Raw JSON, kept only for parsing; the normalised array is authoritative.
    weekday_json: String,
    pub enabled: bool,
    pub label: String,
}

impl LimitRow {
    pub fn to_limit(&self) -> Option<Limit> {
        Some(Limit {
            id: self.id,
            target: self.target?,
            default_minutes: self.default_minutes.max(0) as u32,
            weekday_minutes: self.weekday_minutes,
            enabled: self.enabled,
        })
    }
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

fn kind_from_str(s: &str) -> Option<CategoryKind> {
    match s {
        "limitable" => Some(CategoryKind::Limitable),
        "block_only" => Some(CategoryKind::BlockOnly),
        "never_block" => Some(CategoryKind::NeverBlock),
        _ => None,
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
    fn day_summary_sums_apps_and_primary_categories_only() {
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
        let games = db
            .upsert_app(
                &AppKey::windows_exe("C:\\steam.exe"),
                "Steam",
                None,
                uncat,
                now(),
            )
            .expect("app");
        db.set_app_categories(games, db.category_id("games").expect("games"), &[], true)
            .expect("classify");

        db.record_interval(&interval(tiktok, 600, day)).expect("i1");
        db.record_interval(&interval(games, 1200, day)).expect("i2");

        let summary = db.day_summary(day).expect("summary");
        assert_eq!(summary.total_seconds, 1800);
        assert_eq!(summary.apps.len(), 2);
        assert_eq!(summary.apps[0].label, "Steam", "sorted by seconds desc");
        assert_eq!(summary.apps[0].seconds, 1200);

        // Categories count primary only: TikTok's short-form tag must not
        // double-count, so the two rows sum to the total.
        let cat_sum: i64 = summary.categories.iter().map(|c| c.seconds).sum();
        assert_eq!(cat_sum, 1800);
        assert_eq!(summary.categories.len(), 2);
    }

    #[test]
    fn day_summary_with_no_usage_is_empty_not_error() {
        let db = Db::open_in_memory().expect("open");
        let summary = db.day_summary(DayKey(20260820)).expect("summary");
        assert_eq!(summary.total_seconds, 0);
        assert!(summary.apps.is_empty());
        assert!(summary.categories.is_empty());
    }

    #[test]
    fn pending_loosening_stays_out_of_limits_until_it_promotes() {
        let mut db = Db::open_in_memory().expect("open");
        let social = db.category_id("social-media").expect("social");
        let target = LimitTarget::Category(social);
        let t0 = now();
        db.upsert_limit(&Limit::new(1, target, 30), t0)
            .expect("initial");

        let later = t0 + chrono::Duration::hours(25);
        db.queue_pending_update(&target, 120, [None; 7], true, later)
            .expect("queue loosening");

        // Before the effective time the old limit is still what loads.
        let before = db.load_limits().expect("load");
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].default_minutes, 30);

        db.promote_pending_limits(later).expect("promote");
        let after = db.load_limits().expect("load");
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].default_minutes, 120);
    }

    #[test]
    fn pending_delete_removes_the_limit_when_it_promotes() {
        let mut db = Db::open_in_memory().expect("open");
        let target = LimitTarget::Total;
        let t0 = now();
        db.upsert_limit(&Limit::new(1, target, 60), t0)
            .expect("initial");

        let later = t0 + chrono::Duration::hours(25);
        db.queue_pending_delete(&target, later).expect("queue");

        // Still present before the delete takes effect.
        assert_eq!(db.load_limits().expect("load").len(), 1);
        let pending: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM pending_limits", [], |r| r.get(0))
            .expect("pending count");
        assert_eq!(pending, 1, "pending delete should be queued");
        db.promote_pending_limits(later).expect("promote");
        let after = db.load_limits().expect("load");
        let pending_after: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM pending_limits", [], |r| r.get(0))
            .expect("pending after count");
        assert_eq!(pending_after, 0, "pending row should be consumed");
        assert!(
            after.is_empty(),
            "expected no limits after promote, got {after:?}"
        );
    }

    #[test]
    fn grant_override_is_recorded_and_read_back_by_the_snapshot() {
        let db = Db::open_in_memory().expect("open");
        let social = db.category_id("social-media").expect("social");
        let target = LimitTarget::Category(social);
        let day = DayKey(20260820);

        db.grant_override(&target, day, 15 * 60, now(), Some("test"))
            .expect("grant");
        let snap = db.day_snapshot(day).expect("snapshot");
        assert_eq!(snap.granted_extra_secs(&target), 15 * 60);
    }

    #[test]
    fn block_state_set_clear_and_list_round_trip() {
        let db = Db::open_in_memory().expect("open");
        let app = 7;
        let subject = SubjectRef::App(app);
        let expires = now() + chrono::Duration::hours(4);

        assert!(!db.is_blocked(subject).expect("not blocked"));
        db.set_block(subject, "limit", now(), Some(expires))
            .expect("set");
        assert!(db.is_blocked(subject).expect("blocked"));

        let blocked = db.blocked_subjects().expect("list");
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].0, subject);
        assert_eq!(blocked[0].1, "limit");

        db.clear_block(subject).expect("clear");
        assert!(!db.is_blocked(subject).expect("cleared"));
    }

    #[test]
    fn pin_hash_round_trips_through_settings() {
        let db = Db::open_in_memory().expect("open");
        assert_eq!(db.pin_hash().expect("none"), None);
        db.set_pin_hash("$argon2id$v=19$m=19456,t=2,p=1$fake")
            .expect("set");
        assert_eq!(
            db.pin_hash().expect("some").as_deref(),
            Some("$argon2id$v=19$m=19456,t=2,p=1$fake")
        );
    }

    #[test]
    fn app_record_returns_tags_and_classification() {
        let mut db = Db::open_in_memory().expect("open");
        let social = db.category_id("social-media").expect("social");
        let shortform = db.category_id("short-form-video").expect("shortform");
        let uncat = db.category_id("uncategorized").expect("uncat");

        let id = db
            .upsert_app(
                &AppKey::windows_exe("C:\\tiktok.exe"),
                "TikTok",
                None,
                uncat,
                now(),
            )
            .expect("app");
        db.set_app_categories(id, social, &[shortform], false)
            .expect("classify");

        let record = db.app_record(id).expect("record").expect("some");
        assert_eq!(record.primary_category, social);
        assert_eq!(record.tags, vec![shortform]);
        assert!(!record.user_classified);

        assert!(db.app_record(9999).expect("none").is_none());
    }

    #[test]
    fn list_categories_exposes_kinds_for_the_editor() {
        let db = Db::open_in_memory().expect("open");
        let cats = db.list_categories().expect("list");
        let dev = cats.iter().find(|c| c.slug == "development").expect("dev");
        assert_eq!(dev.kind, CategoryKind::NeverBlock);
        let games = cats.iter().find(|c| c.slug == "games").expect("games");
        assert_eq!(games.kind, CategoryKind::Limitable);
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
