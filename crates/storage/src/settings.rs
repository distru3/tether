//! Settings, the PIN hash, and the append-only audit log.
//!
//! WHY a settings table instead of a config file: settings live under the
//! same SYSTEM/root ACLs as the schema, so "delete the config to reset the
//! PIN" is not an option. Values are plain strings; typed accessors parse on
//! read. `audit` is append-only by convention — nothing in this crate ever
//! updates or deletes from `audit_log`.

use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};

use crate::{Db, Result};

impl Db {
    /// Generic string setting; `None` means unset.
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

    /// Typed accessor with a fallback: an unset or unparsable value yields
    /// `fallback`, so a corrupt row degrades to the shipped default rather
    /// than failing every startup.
    pub fn setting_i64(&self, key: &str, fallback: i64) -> i64 {
        self.setting(key)
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(fallback)
    }

    /// PIN hash, if one has been set. `None` means "no PIN configured", which
    /// in M2 lets limit edits happen without one (see the plan decision).
    pub fn pin_hash(&self) -> Result<Option<String>> {
        self.setting("pin_hash")
    }

    pub fn set_pin_hash(&self, hash: &str) -> Result<()> {
        self.set_setting("pin_hash", hash)
    }

    /// Append to the append-only audit log.
    pub fn audit(&self, now: DateTime<Utc>, kind: &str, detail: Option<&str>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO audit_log (ts_utc, kind, detail) VALUES (?1, ?2, ?3)",
            params![now.to_rfc3339(), kind, detail],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Db;

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
