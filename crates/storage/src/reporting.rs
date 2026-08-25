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

use rusqlite::params;
use st_core::category::CategoryKind;
use st_core::daykey::DayKey;
use st_core::limits::{LimitTarget, UsageSnapshot};

use super::{row_to_target, Db, Result};
use crate::taxonomy::kind_to_str;

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

impl UsageSnapshot for DaySnapshot {
    fn seconds_used(&self, target: &LimitTarget) -> i64 {
        self.used.get(target).copied().unwrap_or(0)
    }

    fn granted_extra_secs(&self, target: &LimitTarget) -> i64 {
        self.granted.get(target).copied().unwrap_or(0)
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
}
