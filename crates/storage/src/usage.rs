//! Usage capture: raw foreground intervals and the daily rollup.
//!
//! WHY two tables for one fact: every dashboard query and limit check reads
//! `usage_daily` (O(days) rows) instead of aggregating `usage_intervals`
//! (O(samples)). `record_interval` writes both in one transaction so the
//! rollup can never drift from the raw data; [`Db::rollup_day`] exists only
//! as a repair path.

use rusqlite::params;
use st_core::daykey::DayKey;
use st_core::model::UsageInterval;

use crate::{subject_to_row, Db, Result};

impl Db {
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
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use st_core::daykey::DayKey;
    use st_core::limits::{LimitTarget, UsageSnapshot};
    use st_core::model::{AppKey, SubjectRef, UsageInterval};

    use super::*;
    use crate::testutil::now;

    fn interval_at(app_id: i64, secs: i64, day: DayKey, start: DateTime<Utc>) -> UsageInterval {
        UsageInterval {
            subject: SubjectRef::App(app_id),
            session_id: "s1".into(),
            start,
            end: start + chrono::Duration::seconds(secs),
            day_key: day,
        }
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

        db.record_interval(&interval_at(tiktok, 600, day, now()))
            .expect("i1");
        db.record_interval(&interval_at(tiktok, 300, day, now()))
            .expect("i2");

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
        db.record_interval(&interval_at(term, 3600, day, now()))
            .expect("i1");

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

        db.record_interval(&interval_at(app, 120, day, now()))
            .expect("i1");
        db.record_interval(&interval_at(
            app,
            240,
            day,
            now() + chrono::Duration::minutes(5),
        ))
        .expect("i2");
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
}
