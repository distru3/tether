//! Reporting: everything the dashboards and the limits engine read.
//!
//! Two shapes come out of here, deliberately different:
//!
//! * [`Db::day_snapshot`] is the limits engine's view — per-target maps of
//!   used and granted seconds, with category usage computed through the
//!   `app_categories` view so a tagged app counts toward *every* category
//!   that limits it, and never-blockable apps excluded from the total.
//! * [`Db::day_summary`] is the dashboard's view — sorted rows for rendering,
//!   counting primary categories only so category bars sum to 100%.
//!
//! Storage returns plain structs; the agent maps them to IPC DTOs. That keeps
//! this crate free of any dependency on `st-ipc`.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use rusqlite::params;
use st_core::category::CategoryKind;
use st_core::daykey::DayKey;
use st_core::limits::{LimitTarget, UsageSnapshot};

use super::{row_to_target, Db, Result, StorageError};
use crate::taxonomy::kind_to_str;

/// One day's usage, ready to feed the limits engine.
pub struct DaySnapshot {
    pub day: DayKey,
    pub used: HashMap<LimitTarget, i64>,
    pub timer_expires_utc: HashMap<LimitTarget, DateTime<Utc>>,
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

/// One day's grand total inside a [`WeeklySummary`].
pub struct DailyTotal {
    pub day: DayKey,
    pub total_seconds: i64,
}

/// The weekly dashboard view: seven zero-filled daily totals plus the
/// previous week's grand total. Storage returns this; the agent maps it to
/// the IPC `WeeklySummaryDto` so the storage crate never depends on
/// `st-ipc`.
pub struct WeeklySummary {
    /// Exactly seven entries, oldest first. Days without usage carry 0 so
    /// the shape is constant and the UI never has to hole-fill a chart.
    pub days: Vec<DailyTotal>,
    /// Total seconds for the 7 days immediately before the reported week,
    /// for the week-over-week comparison.
    pub previous_week_total: i64,
}

impl UsageSnapshot for DaySnapshot {
    fn seconds_used(&self, target: &LimitTarget) -> i64 {
        self.used.get(target).copied().unwrap_or(0)
    }

    fn active_timer_expires_utc(&self, target: &LimitTarget) -> Option<DateTime<Utc>> {
        self.timer_expires_utc.get(target).copied()
    }
}

impl Db {
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
        // not eat the budget for everything else. The kind literal comes from
        // the single codec rather than being spelled out again here.
        let total: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(ud.seconds), 0)
             FROM usage_daily ud
             JOIN apps a ON a.id = ud.subject_id
             JOIN categories c ON c.id = a.primary_category_id
             WHERE ud.subject_type = 'app'
               AND ud.day_key = ?1
               AND c.kind != ?2",
            params![day.0, kind_to_str(CategoryKind::NeverBlock)],
            |row| row.get(0),
        )?;
        used.insert(LimitTarget::Total, total);

        // Active overrides (wall-clock timers).
        let mut timer_expires_utc: HashMap<LimitTarget, DateTime<Utc>> = HashMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT target_type, target_id, MAX(expires_utc)
             FROM overrides WHERE day_key = ?1
             GROUP BY target_type, target_id",
        )?;
        for row in stmt.query_map(params![day.0], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })? {
            let (target_type, target_id, expires_str) = row?;
            if let Some(target) = row_to_target(&target_type, target_id) {
                if let Some(s) = expires_str {
                    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
                        timer_expires_utc.insert(target, dt);
                    }
                }
            }
        }

        Ok(DaySnapshot {
            day,
            used,
            timer_expires_utc,
        })
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

    /// Weekly dashboard totals: the 7 days ending at `end_day` (inclusive)
    /// plus the previous week's grand total.
    ///
    /// Date arithmetic goes through `NaiveDate` (`DayKey` → date → `DayKey`)
    /// because subtracting from the `YYYYMMDD` *integer* is wrong across
    /// every month boundary. All chrono math is checked: a wire-supplied
    /// `DayKey` near the edges of the representable calendar must degrade
    /// to [`StorageError::InvalidDay`], never panic an agent worker.
    pub fn weekly_summary(&self, end_day: DayKey) -> Result<WeeklySummary> {
        let invalid = || StorageError::InvalidDay(end_day.0);
        let end_date = end_day.to_date().ok_or_else(invalid)?;
        let start_date = end_date
            .checked_sub_signed(Duration::days(6))
            .ok_or_else(invalid)?;
        let prev_end_day = DayKey::from_date(
            start_date
                .checked_sub_signed(Duration::days(1))
                .ok_or_else(invalid)?,
        );
        let prev_start_day = DayKey::from_date(
            start_date
                .checked_sub_signed(Duration::days(7))
                .ok_or_else(invalid)?,
        );

        // Days that have rows, with their per-day totals. `BETWEEN` on the
        // encoded keys is correct range math: YYYYMMDD sorts chronologically.
        let mut present: HashMap<DayKey, i64> = HashMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT day_key, COALESCE(SUM(seconds), 0) FROM usage_daily
             WHERE subject_type = 'app' AND day_key BETWEEN ?1 AND ?2
             GROUP BY day_key",
        )?;
        for row in stmt.query_map(params![DayKey::from_date(start_date).0, end_day.0], |r| {
            Ok((DayKey(r.get::<_, i32>(0)?), r.get::<_, i64>(1)?))
        })? {
            let (day, secs) = row?;
            present.insert(day, secs);
        }

        // Zero-fill: walk the 7 calendar days oldest-first. start+6 days is
        // known valid (it is `end_date`), so the additions cannot overflow in
        // practice — checked anyway so absurd inputs stay errors, not panics.
        let mut days = Vec::with_capacity(7);
        for offset in 0..7 {
            let date = start_date
                .checked_add_signed(Duration::days(offset))
                .ok_or_else(invalid)?;
            let day = DayKey::from_date(date);
            let total_seconds = present.get(&day).copied().unwrap_or(0);
            days.push(DailyTotal { day, total_seconds });
        }

        let previous_week_total: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(seconds), 0) FROM usage_daily
             WHERE subject_type = 'app' AND day_key BETWEEN ?1 AND ?2",
            params![prev_start_day.0, prev_end_day.0],
            |row| row.get(0),
        )?;

        Ok(WeeklySummary {
            days,
            previous_week_total,
        })
    }
}

#[cfg(test)]
mod tests {
    use st_core::daykey::DayKey;
    use st_core::model::AppKey;

    use super::*;
    use crate::testutil::{interval, now};

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
    fn weekly_summary_zero_fills_seven_days_and_totals_the_previous_week_across_a_month_boundary() {
        let mut db = Db::open_in_memory().expect("open");
        let uncat = db.category_id("uncategorized").expect("uncat");
        let app = db
            .upsert_app(
                &AppKey::windows_exe("C:\\steam.exe"),
                "Steam",
                None,
                uncat,
                now(),
            )
            .expect("app");

        // Ten days ending 2026-09-03. The reported week is 08-28..09-03 —
        // crossing the August→September boundary, which naive subtraction on
        // the YYYYMMDD integer would get wrong — and 08-25..27 sit in the
        // previous week. Only some days carry usage; the rest prove the
        // zero-fill.
        db.record_interval(&interval(app, 1200, DayKey(20260825)))
            .expect("prev week mon");
        db.record_interval(&interval(app, 600, DayKey(20260827)))
            .expect("prev week wed");
        db.record_interval(&interval(app, 900, DayKey(20260829)))
            .expect("sat");
        db.record_interval(&interval(app, 300, DayKey(20260901)))
            .expect("september");
        db.record_interval(&interval(app, 1500, DayKey(20260903)))
            .expect("end day");

        let summary = db.weekly_summary(DayKey(20260903)).expect("summary");

        assert_eq!(summary.days.len(), 7, "always exactly seven days");
        let totals: Vec<(DayKey, i64)> = summary
            .days
            .iter()
            .map(|d| (d.day, d.total_seconds))
            .collect();
        assert_eq!(
            totals,
            vec![
                (DayKey(20260828), 0),
                (DayKey(20260829), 900),
                (DayKey(20260830), 0),
                (DayKey(20260831), 0),
                (DayKey(20260901), 300),
                (DayKey(20260902), 0),
                (DayKey(20260903), 1500),
            ],
            "oldest first, gaps zero-filled, month boundary crossed correctly"
        );
        assert_eq!(summary.previous_week_total, 1800);
    }

    #[test]
    fn weekly_summary_with_no_usage_zero_fills_every_day() {
        let db = Db::open_in_memory().expect("open");
        let summary = db.weekly_summary(DayKey(20260820)).expect("summary");
        assert_eq!(summary.days.len(), 7);
        assert!(
            summary.days.iter().all(|d| d.total_seconds == 0),
            "no usage anywhere means zero bars, not missing bars"
        );
        assert_eq!(summary.previous_week_total, 0);
        // The window starts six days before the end day.
        assert_eq!(
            summary.days.first().expect("seven entries").day,
            DayKey(20260814)
        );
        assert_eq!(
            summary.days.last().expect("seven entries").day,
            DayKey(20260820)
        );
    }

    #[test]
    fn weekly_summary_rejects_an_unparsable_day_key() {
        let db = Db::open_in_memory().expect("open");
        // February 30th is not a day; the range cannot be computed honestly.
        assert!(matches!(
            db.weekly_summary(DayKey(20260230)),
            Err(StorageError::InvalidDay(20260230))
        ));
    }
}
