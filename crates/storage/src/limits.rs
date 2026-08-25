//! Limits: the live `limits` table plus the anti-impulse `pending_limits`
//! queue.
//!
//! The cooldown story in one place: *tightening* goes straight into `limits`
//! via [`Db::upsert_limit`]; *loosening* or removal is queued in
//! `pending_limits` with an `effective_from_utc`, and [`Db::promote_pending_limits`]
//! moves queued rows over once their time has come. Until then the tighter
//! value keeps being enforced.
//!
//! The weekday JSON codec at the bottom of this file is the single converter
//! between the `weekday_minutes` TEXT column and `[Option<u32>; 7]`. Every
//! reader normalises through it, so a short array warns once and behaves
//! identically everywhere instead of being silently truncated three
//! different ways.

use chrono::{DateTime, Utc};
use rusqlite::params;
use st_core::limits::{Limit, LimitTarget};

use super::{row_to_target, target_to_row, Db, Result, StorageError};

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

/// A limit plus its human-readable label, for the editor UI.
pub struct LimitRow {
    pub id: i64,
    pub target: Option<LimitTarget>,
    pub default_minutes: i64,
    /// Normalised through the single weekday codec; authoritative.
    pub weekday_minutes: [Option<u32>; 7],
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

/// Parse the `weekday_minutes` column into the canonical 7-slot array.
///
/// THE weekday codec: all three readers (load_limits, list_limit_rows,
/// promote_pending_limits) go through here. A stored array of the wrong
/// length cannot come from this crate's writers, so it means tampering or
/// corruption: warn loudly, then keep whatever slots exist and pad the rest
/// with `None` ("fall back to the default") rather than failing every
/// subsequent load forever.
fn weekday_minutes_from_json(column: &'static str, json: &str) -> Result<[Option<u32>; 7]> {
    let parsed: Vec<Option<u32>> =
        serde_json::from_str(json).map_err(|source| StorageError::Json { column, source })?;
    if parsed.len() != 7 {
        tracing::warn!(
            column,
            stored_len = parsed.len(),
            "weekday_minutes array has unexpected length; padding/truncating"
        );
    }
    let mut out = [None; 7];
    for (slot, value) in out.iter_mut().zip(parsed) {
        *slot = value;
    }
    Ok(out)
}

/// Serialise the canonical array back to column form.
fn weekday_minutes_to_json(weekday: &[Option<u32>; 7]) -> String {
    serde_json::to_string(weekday).expect("array of options always serialises")
}

/// WHERE clause matching exactly one target row. SQL `= NULL` never matches,
/// so the total budget (NULL `target_id`) needs `IS NULL`.
fn target_scope_sql(target_type: &str, target_id: Option<i64>) -> String {
    match target_id {
        Some(tid) => format!("target_type = '{target_type}' AND target_id = {tid}"),
        None => format!("target_type = '{target_type}' AND target_id IS NULL"),
    }
}

impl Db {
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
            out.push(Limit {
                id,
                target,
                default_minutes: default_minutes.max(0) as u32,
                weekday_minutes: weekday_minutes_from_json(
                    "limits.weekday_minutes",
                    &weekday_json,
                )?,
                enabled: enabled != 0,
            });
        }
        Ok(out)
    }

    pub fn upsert_limit(&self, limit: &Limit, now: DateTime<Utc>) -> Result<()> {
        let (target_type, target_id) = target_to_row(&limit.target);
        let weekday_json = weekday_minutes_to_json(&limit.weekday_minutes);
        // UPDATE-then-INSERT instead of ON CONFLICT: a SQL UNIQUE constraint
        // treats NULLs as distinct, so 0001's `UNIQUE (target_type,
        // target_id)` silently never fired for total orders (`target_id IS
        // NULL`) and every edit inserted a duplicate. The expression index
        // from 0003 is the structural guard; this writer no longer depends on
        // conflict detection at all. `IS` matches NULLs where `=` does not.
        let updated = self.conn.execute(
            "UPDATE limits
             SET default_minutes = ?1, weekday_minutes = ?2, enabled = ?3
             WHERE target_type = ?4 AND target_id IS ?5",
            params![
                limit.default_minutes,
                weekday_json,
                limit.enabled as i64,
                target_type,
                target_id,
            ],
        )?;
        if updated == 0 {
            self.conn.execute(
                "INSERT INTO limits
                     (target_type, target_id, default_minutes, weekday_minutes, enabled, created_utc)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    target_type,
                    target_id,
                    limit.default_minutes,
                    weekday_json,
                    limit.enabled as i64,
                    now.to_rfc3339(),
                ],
            )?;
        }
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
        let sql = format!(
            "DELETE FROM limits WHERE {}",
            target_scope_sql(target_type, target_id)
        );
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
        let weekday_json = weekday_minutes_to_json(&weekday_minutes);
        // Same NULL-conflict story as `upsert_limit`: UPDATE-then-INSERT with
        // `IS`, so re-queuing a total-order change replaces the queued row.
        let updated = self.conn.execute(
            "UPDATE pending_limits
             SET action = 'update', default_minutes = ?1, weekday_minutes = ?2,
                 enabled = ?3, effective_from_utc = ?4
             WHERE target_type = ?5 AND target_id IS ?6",
            params![
                default_minutes,
                weekday_json,
                enabled as i64,
                effective_from_utc.to_rfc3339(),
                target_type,
                target_id,
            ],
        )?;
        if updated == 0 {
            self.conn.execute(
                "INSERT INTO pending_limits
                     (target_type, target_id, action, default_minutes, weekday_minutes,
                      enabled, effective_from_utc)
                 VALUES (?1, ?2, 'update', ?3, ?4, ?5, ?6)",
                params![
                    target_type,
                    target_id,
                    default_minutes,
                    weekday_json,
                    enabled as i64,
                    effective_from_utc.to_rfc3339(),
                ],
            )?;
        }
        Ok(())
    }

    /// Queue a limit removal that takes effect at `effective_from_utc`.
    pub fn queue_pending_delete(
        &self,
        target: &LimitTarget,
        effective_from_utc: DateTime<Utc>,
    ) -> Result<()> {
        let (target_type, target_id) = target_to_row(target);
        let updated = self.conn.execute(
            "UPDATE pending_limits
             SET action = 'delete', default_minutes = NULL, weekday_minutes = NULL,
                 enabled = NULL, effective_from_utc = ?1
             WHERE target_type = ?2 AND target_id IS ?3",
            params![effective_from_utc.to_rfc3339(), target_type, target_id],
        )?;
        if updated == 0 {
            self.conn.execute(
                "INSERT INTO pending_limits
                     (target_type, target_id, action, effective_from_utc)
                 VALUES (?1, ?2, 'delete', ?3)",
                params![target_type, target_id, effective_from_utc.to_rfc3339()],
            )?;
        }
        Ok(())
    }

    /// Move any pending change whose time has come into the live `limits`
    /// table. Called by the enforcement loop every tick so a loosened limit
    /// starts applying the instant its cooldown elapses.
    ///
    /// Rows with an unknown target encoding are deleted rather than left to
    /// trip promotion on every future tick; malformed weekday JSON fails the
    /// whole batch (same as before), while a wrong-length array is normalised
    /// by the shared codec instead of skipping the update.
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
            let scope = target_scope_sql(tt, tid);
            match action.as_str() {
                "delete" => {
                    let sql = format!("DELETE FROM limits WHERE {scope}");
                    tx.execute(&sql, [])?;
                }
                "update" => {
                    let weekday_json = weekday_json.unwrap_or_default();
                    let weekday =
                        weekday_minutes_from_json("pending_limits.weekday_minutes", &weekday_json)?;
                    let sql = format!(
                        "UPDATE limits SET
                             default_minutes = ?1, weekday_minutes = ?2, enabled = ?3
                         WHERE {scope}"
                    );
                    tx.execute(
                        &sql,
                        params![default_minutes, weekday_minutes_to_json(&weekday), enabled],
                    )?;
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
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)? != 0,
                row.get::<_, String>(6)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, target_type, target_id, default_minutes, weekday_json, enabled, label) = row?;
            let Some(target) = row_to_target(&target_type, target_id) else {
                tracing::warn!(id, target_type, "skipping limit with unknown target type");
                continue;
            };
            // Same codec as `load_limits`, so the editor sees exactly what the
            // enforcement loop sees.
            let weekday_minutes =
                weekday_minutes_from_json("limits.weekday_minutes", &weekday_json)?;
            out.push(LimitRow {
                id,
                target: Some(target),
                default_minutes,
                weekday_minutes,
                enabled,
                label,
            });
        }
        Ok(out)
    }

    /// Raw access for tests that need to simulate corrupted columns.
    #[cfg(test)]
    fn set_raw_weekday_json(&self, id: i64, json: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE limits SET weekday_minutes = ?2 WHERE id = ?1",
            params![id, json],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use st_core::daykey::DayKey;
    use st_core::limits::LimitEngine;
    use st_core::model::AppKey;

    use super::*;
    use crate::testutil::{interval, now};

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

    /// Regression: SQL UNIQUE treats NULLs as distinct, so re-saving the
    /// *total* order used to insert a second row on every suspend/edit. The
    /// upsert must replace per target across all three kinds.
    #[test]
    fn upserting_any_target_twice_stays_one_row() {
        let db = Db::open_in_memory().expect("open");
        let games = db.category_id("games").expect("games");

        db.upsert_limit(&Limit::new(0, LimitTarget::Total, 60), now())
            .expect("total v1");
        db.upsert_limit(&Limit::new(0, LimitTarget::Total, 45), now())
            .expect("total v2 (was duplicating)");

        db.upsert_limit(&Limit::new(0, LimitTarget::App(7), 30), now())
            .expect("app v1");
        db.upsert_limit(&Limit::new(0, LimitTarget::App(7), 20), now())
            .expect("app v2");

        db.upsert_limit(&Limit::new(0, LimitTarget::Category(games), 90), now())
            .expect("category v1");
        db.upsert_limit(&Limit::new(0, LimitTarget::Category(games), 80), now())
            .expect("category v2");

        let loaded = db.load_limits().expect("load");
        assert_eq!(loaded.len(), 3, "one row per target, not one per save");
        let total = loaded.iter().find(|l| l.target == LimitTarget::Total);
        assert_eq!(total.map(|l| l.default_minutes), Some(45));
    }

    #[test]
    fn queueing_a_pending_change_twice_keeps_one_row_even_for_total() {
        let mut db = Db::open_in_memory().expect("open");
        let later = now() + chrono::Duration::hours(24);

        db.queue_pending_update(&LimitTarget::Total, 120, [None; 7], true, later)
            .expect("queue v1");
        db.queue_pending_update(&LimitTarget::Total, 150, [None; 7], true, later)
            .expect("queue v2 (was duplicating)");
        db.queue_pending_delete(&LimitTarget::Total, later)
            .expect("re-queue as delete replaces the update");

        let pending: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM pending_limits", [], |r| r.get(0))
            .expect("count");
        let action: String = db
            .conn()
            .query_row("SELECT action FROM pending_limits", [], |r| r.get(0))
            .expect("action");
        assert_eq!(pending, 1, "one queued change per target");
        assert_eq!(action, "delete", "the newest queued action wins");
        // And promotion still consumes it cleanly.
        db.promote_pending_limits(later).expect("promote");
        let pending_after: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM pending_limits", [], |r| r.get(0))
            .expect("count after");
        assert_eq!(pending_after, 0);
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

    /// A short stored array must not fail the load nor silently vanish: it
    /// warns once and pads the missing slots with `None` (= use the daily
    /// default). This pins down the ONE weekday codec all readers share.
    #[test]
    fn load_limits_warns_and_normalises_a_short_weekday_array() {
        let db = Db::open_in_memory().expect("open");
        let social = db.category_id("social-media").expect("social");
        let limit = Limit::new(1, LimitTarget::Category(social), 45);
        db.upsert_limit(&limit, now()).expect("insert");
        let id = db.load_limits().expect("load")[0].id;

        // Bypass the writer to simulate corruption/tampering.
        db.set_raw_weekday_json(id, "[90]").expect("corrupt");

        let loaded = db.load_limits().expect("still loads");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].weekday_minutes[0], Some(90));
        assert!(
            loaded[0].weekday_minutes[1..]
                .iter()
                .all(|slot| slot.is_none()),
            "missing slots pad with None"
        );

        // The editor view normalises through the same codec.
        let rows = db.list_limit_rows().expect("rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].weekday_minutes[0], Some(90));
    }
}
