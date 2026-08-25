//! Enforcement state: blocks and PIN-approved overrides.
//!
//! Everything here is *state the enforcement loop acts on right now*: which
//! subjects are frozen, and how much bonus time a parent has granted for a
//! given day. Overrides are additive per `day_key` so they expire with
//! everything else at the day boundary; blocks are replaced on re-block so a
//! re-freeze always moves the deadline forward.

use chrono::{DateTime, Utc};
use rusqlite::params;
use st_core::daykey::DayKey;
use st_core::limits::LimitTarget;
use st_core::model::SubjectRef;

use super::{subject_from_row, subject_to_row, target_to_row, Db, Result};

impl Db {
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
    ///
    /// A row whose `subject_type` no code in this workspace writes cannot be
    /// interpreted; it is skipped with a warning instead of failing the whole
    /// listing (which would strand every other block until thawing breaks).
    pub fn blocked_subjects(&self) -> Result<Vec<(SubjectRef, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT subject_type, subject_id, reason FROM block_state")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (kind, id, reason) = row?;
            let Some(subject) = subject_from_row(&kind, id) else {
                tracing::warn!(
                    subject_type = %kind,
                    "skipping block_state row with unknown subject type"
                );
                continue;
            };
            out.push((subject, reason));
        }
        Ok(out)
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
}

#[cfg(test)]
mod tests {
    use st_core::limits::{LimitTarget, UsageSnapshot};
    use st_core::model::SubjectRef;

    use super::*;
    use crate::testutil::now;

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

    /// The production schema CHECKs `subject_type`, so simulate a future or
    /// tampered row by recreating the table without the constraint. One bad
    /// row must not abort the listing of all the good ones.
    #[test]
    fn blocked_subjects_skips_unknown_subject_types_and_keeps_the_rest() {
        let db = Db::open_in_memory().expect("open");
        db.conn()
            .execute_batch(
                "DROP TABLE block_state;
                 CREATE TABLE block_state (
                   subject_type      TEXT NOT NULL,
                   subject_id        INTEGER NOT NULL,
                   reason            TEXT NOT NULL,
                   blocked_since_utc TEXT NOT NULL,
                   expires_utc       TEXT,
                   PRIMARY KEY (subject_type, subject_id)
                 ) WITHOUT ROWID;",
            )
            .expect("recreate without CHECK");

        let good = SubjectRef::App(7);
        db.set_block(good, "limit", now(), None).expect("good row");
        db.conn()
            .execute(
                "INSERT INTO block_state
                     (subject_type, subject_id, reason, blocked_since_utc, expires_utc)
                 VALUES ('device', 3, 'why', '2026-08-20T00:00:00Z', NULL)",
                [],
            )
            .expect("row no writer could have produced");

        let blocked = db.blocked_subjects().expect("skips, does not fail");
        assert_eq!(blocked.len(), 1, "only the interpretable row survives");
        assert_eq!(blocked[0].0, good);
        assert_eq!(blocked[0].1, "limit");

        // is_blocked is unaffected by the unknown row.
        assert!(db.is_blocked(good).expect("good still blocked"));
    }
}
