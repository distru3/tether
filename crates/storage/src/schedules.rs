//! Downtime schedules and always-allowed subject management.
//!
//! # Schema
//!
//! Uses the existing `schedules` and `allowlist` tables defined in `0001_init.sql`.

use rusqlite::params;
use st_core::schedules::DowntimeSchedule;

use super::{Db, Result, StorageError};

impl Db {
    /// Create a new downtime schedule.
    pub fn create_schedule(
        &self,
        name: &str,
        weekday_mask: u8,
        start_minute: u32,
        end_minute: u32,
    ) -> Result<i64> {
        if start_minute >= 1440 || end_minute >= 1440 {
            return Err(StorageError::Invalid(format!(
                "start_minute and end_minute must be between 0 and 1439 (got {start_minute}, {end_minute})"
            )));
        }

        self.conn.execute(
            "INSERT INTO schedules (name, weekday_mask, start_minute, end_minute, enabled)
             VALUES (?1, ?2, ?3, ?4, 1)",
            params![name, weekday_mask, start_minute, end_minute],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// List all downtime schedules.
    pub fn list_schedules(&self) -> Result<Vec<DowntimeSchedule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, weekday_mask, start_minute, end_minute, enabled
             FROM schedules ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let name: String = row.get(1)?;
            let weekday_mask_i: i64 = row.get(2)?;
            let start_minute: u32 = row.get(3)?;
            let end_minute: u32 = row.get(4)?;
            let enabled: i64 = row.get(5)?;
            Ok(DowntimeSchedule {
                id,
                name,
                weekday_mask: weekday_mask_i as u8,
                start_minute,
                end_minute,
                enabled: enabled != 0,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Update an existing downtime schedule.
    pub fn update_schedule(
        &self,
        id: i64,
        name: &str,
        weekday_mask: u8,
        start_minute: u32,
        end_minute: u32,
    ) -> Result<()> {
        if start_minute >= 1440 || end_minute >= 1440 {
            return Err(StorageError::Invalid(format!(
                "start_minute and end_minute must be between 0 and 1439 (got {start_minute}, {end_minute})"
            )));
        }

        let affected = self.conn.execute(
            "UPDATE schedules SET name = ?1, weekday_mask = ?2, start_minute = ?3, end_minute = ?4
             WHERE id = ?5",
            params![name, weekday_mask, start_minute, end_minute, id],
        )?;
        if affected == 0 {
            return Err(StorageError::Invalid(format!("schedule {id} not found")));
        }
        Ok(())
    }

    /// Enable or disable a downtime schedule.
    pub fn set_schedule_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        let affected = self.conn.execute(
            "UPDATE schedules SET enabled = ?1 WHERE id = ?2",
            params![if enabled { 1 } else { 0 }, id],
        )?;
        if affected == 0 {
            return Err(StorageError::Invalid(format!("schedule {id} not found")));
        }
        Ok(())
    }

    /// Delete a downtime schedule.
    pub fn delete_schedule(&self, id: i64) -> Result<()> {
        let affected = self
            .conn
            .execute("DELETE FROM schedules WHERE id = ?1", params![id])?;
        if affected == 0 {
            return Err(StorageError::Invalid(format!("schedule {id} not found")));
        }
        Ok(())
    }

    /// Add or remove a subject from the downtime allowlist.
    pub fn set_allowlist_subject(
        &self,
        subject_type: &str,
        subject_id: i64,
        allowed: bool,
    ) -> Result<()> {
        if subject_type != "app" && subject_type != "site" {
            return Err(StorageError::Invalid(format!(
                "invalid subject_type {subject_type} (expected 'app' or 'site')"
            )));
        }

        if allowed {
            self.conn.execute(
                "INSERT OR IGNORE INTO allowlist (subject_type, subject_id) VALUES (?1, ?2)",
                params![subject_type, subject_id],
            )?;
        } else {
            self.conn.execute(
                "DELETE FROM allowlist WHERE subject_type = ?1 AND subject_id = ?2",
                params![subject_type, subject_id],
            )?;
        }
        Ok(())
    }

    /// List all allowlisted subjects `(subject_type, subject_id)`.
    pub fn list_allowlist(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT subject_type, subject_id FROM allowlist")?;
        let rows = stmt.query_map([], |row| {
            let kind: String = row.get(0)?;
            let id: i64 = row.get(1)?;
            Ok((kind, id))
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Check if a subject is in the downtime allowlist.
    pub fn is_subject_allowlisted(&self, subject_type: &str, subject_id: i64) -> Result<bool> {
        let mut stmt = self.conn.prepare(
            "SELECT 1 FROM allowlist WHERE subject_type = ?1 AND subject_id = ?2 LIMIT 1",
        )?;
        let exists = stmt.exists(params![subject_type, subject_id])?;
        Ok(exists)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_crud_round_trip() {
        let db = Db::open_in_memory().unwrap();

        // 1. Create
        let id = db
            .create_schedule("Bedtime", 0b01111111, 1320, 420)
            .unwrap();
        assert!(id > 0);

        // 2. List
        let list = db.list_schedules().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Bedtime");
        assert_eq!(list[0].weekday_mask, 0b01111111);
        assert_eq!(list[0].start_minute, 1320);
        assert_eq!(list[0].end_minute, 420);
        assert!(list[0].enabled);

        // 3. Update
        db.update_schedule(id, "Night Time", 0b00011111, 1380, 360)
            .unwrap();
        let list = db.list_schedules().unwrap();
        assert_eq!(list[0].name, "Night Time");
        assert_eq!(list[0].weekday_mask, 0b00011111);
        assert_eq!(list[0].start_minute, 1380);

        // 4. Toggle
        db.set_schedule_enabled(id, false).unwrap();
        let list = db.list_schedules().unwrap();
        assert!(!list[0].enabled);

        // 5. Delete
        db.delete_schedule(id).unwrap();
        assert!(db.list_schedules().unwrap().is_empty());
    }

    #[test]
    fn allowlist_crud_round_trip() {
        let db = Db::open_in_memory().unwrap();

        assert!(!db.is_subject_allowlisted("app", 10).unwrap());

        db.set_allowlist_subject("app", 10, true).unwrap();
        assert!(db.is_subject_allowlisted("app", 10).unwrap());

        let list = db.list_allowlist().unwrap();
        assert_eq!(list, vec![("app".into(), 10)]);

        db.set_allowlist_subject("app", 10, false).unwrap();
        assert!(!db.is_subject_allowlisted("app", 10).unwrap());
        assert!(db.list_allowlist().unwrap().is_empty());
    }
}
