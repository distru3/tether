//! Block-screen reasons (`0005_block_reasons.sql`).

use chrono::{DateTime, Utc};
use rusqlite::params;
use st_core::daykey::DayKey;
use st_core::reasons::BlockReason;

use super::{Db, Result};

impl Db {
    /// Record (or replace) the reason given for `app_id` on `day`.
    pub fn record_block_reason(
        &self,
        day: DayKey,
        app_id: i64,
        reason: BlockReason,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO block_reasons (day_key, app_id, reason, recorded_utc)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (day_key, app_id) DO UPDATE
               SET reason = excluded.reason, recorded_utc = excluded.recorded_utc",
            params![day.0, app_id, reason.as_str(), now.to_rfc3339()],
        )?;
        Ok(())
    }

    /// How often each reason was given between `from` and `to` (inclusive),
    /// most common first. Reasons never given are left out.
    pub fn block_reason_counts(&self, from: DayKey, to: DayKey) -> Result<Vec<(BlockReason, u32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT reason, COUNT(*) FROM block_reasons
             WHERE day_key BETWEEN ?1 AND ?2
             GROUP BY reason ORDER BY COUNT(*) DESC, reason ASC",
        )?;
        let rows = stmt.query_map(params![from.0, to.0], |row| {
            let reason: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((reason, count))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (reason, count) = row?;
            // The CHECK constraint keeps unknown values out; skip defensively.
            if let Some(r) = BlockReason::parse(&reason) {
                out.push((r, u32::try_from(count).unwrap_or(u32::MAX)));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use st_core::daykey::DayKey;
    use st_core::model::AppKey;
    use st_core::reasons::BlockReason;

    use crate::Db;

    fn db_with_apps() -> (Db, i64, i64) {
        let db = Db::open_in_memory().expect("open");
        let cat = db.category_id("games").expect("games");
        let at = Utc.with_ymd_and_hms(2026, 10, 1, 9, 0, 0).unwrap();
        let a = db
            .upsert_app(&AppKey::windows_exe("C:\\a.exe"), "A", None, cat, at)
            .expect("insert a");
        let b = db
            .upsert_app(&AppKey::windows_exe("C:\\b.exe"), "B", None, cat, at)
            .expect("insert b");
        (db, a, b)
    }

    #[test]
    fn one_reason_per_app_per_day_and_counts_by_range() {
        let (db, a, b) = db_with_apps();
        let now = Utc.with_ymd_and_hms(2026, 10, 5, 12, 0, 0).unwrap();
        let d1 = DayKey(20261004);
        let d2 = DayKey(20261005);
        db.record_block_reason(d1, a, BlockReason::Finish, now)
            .unwrap();
        // Answering again the same day replaces the first answer.
        db.record_block_reason(d1, a, BlockReason::Habit, now)
            .unwrap();
        db.record_block_reason(d1, b, BlockReason::Habit, now)
            .unwrap();
        db.record_block_reason(d2, a, BlockReason::Bored, now)
            .unwrap();

        assert_eq!(
            db.block_reason_counts(d1, d2).unwrap(),
            vec![(BlockReason::Habit, 2), (BlockReason::Bored, 1)]
        );
        assert_eq!(
            db.block_reason_counts(d2, d2).unwrap(),
            vec![(BlockReason::Bored, 1)]
        );
        assert!(db
            .block_reason_counts(DayKey(20260101), DayKey(20260102))
            .unwrap()
            .is_empty());
    }
}
