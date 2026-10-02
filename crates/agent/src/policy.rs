//! Live, user-editable enforcement policy.
//!
//! One `Arc<RwLock<Policy>>` is shared by the IPC server (which applies
//! `SetSetting` changes to it) and the main loop (which reads it every tick).
//! Previously the main loop captured `day_start_minutes` once at startup while
//! IPC handlers read the live value, so changing the day boundary bucketed new
//! usage under one day while limits were evaluated against another until the
//! agent restarted.

use st_core::settings::SettingKey;
use st_storage::Db;

/// Policy knobs read from the `settings` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// Hours before a loosened limit takes effect.
    pub limit_cooldown_hours: i64,
    /// When true, "+15 minutes" overrides are refused outright.
    pub strict_mode: bool,
    /// Minutes after local midnight at which the day rolls over. Overrides
    /// must be attributed to exactly the day the enforcer/sampler compute.
    pub day_start_minutes: i64,
    /// Idle seconds beyond which focused time stops accruing.
    pub idle_threshold_secs: i64,
    /// Whether to show the remaining time HUD on limited apps.
    pub show_hud_overlay: bool,
    /// Whether to allow the floating HUD over full-screen games (may break driver FPS limits).
    pub show_hud_in_fullscreen: bool,
    /// Remapable global hotkey to peek the HUD when dormant in games (default "Ctrl+Alt+T").
    pub hud_peek_hotkey: String,
    /// Alert chime audio volume from 0 to 100 (default 80).
    pub alert_volume: i64,
}

impl Default for Policy {
    /// The shipped defaults, i.e. [`SettingKey::default_value`] for every key.
    fn default() -> Self {
        let mut policy = Self {
            limit_cooldown_hours: 0,
            strict_mode: false,
            day_start_minutes: 0,
            idle_threshold_secs: 0,
            show_hud_overlay: false,
            show_hud_in_fullscreen: false,
            hud_peek_hotkey: String::new(),
            alert_volume: 0,
        };
        for key in SettingKey::ALL {
            policy.apply(key, key.default_value());
        }
        policy
    }
}

impl Policy {
    /// Load every setting, falling back per key to its default when the row is
    /// missing *or invalid* — a corrupt row degrades to the shipped default
    /// instead of failing startup or poisoning the policy with an out-of-range
    /// value.
    pub fn load(db: &Db) -> Self {
        let mut policy = Self::default();
        for key in SettingKey::ALL {
            let stored = match db.setting(key.storage_key()) {
                Ok(value) => value,
                Err(e) => {
                    tracing::warn!(key = key.wire_name(), error = %e, "setting unreadable; using default");
                    None
                }
            };
            if let Some(raw) = stored {
                match key.normalize(&raw) {
                    Ok(value) => policy.apply(key, &value),
                    Err(reason) => {
                        tracing::warn!(key = key.wire_name(), %reason, "stored setting invalid; using default")
                    }
                }
            }
        }
        policy
    }

    /// Apply one already-normalized value (see [`SettingKey::normalize`]).
    /// Keys without a live policy field (Family DNS, window-title capture) are
    /// ignored here; their effects live elsewhere.
    pub fn apply(&mut self, key: SettingKey, value: &str) {
        let number = || value.parse::<i64>().unwrap_or_default();
        match key {
            SettingKey::LimitCooldownHours => self.limit_cooldown_hours = number(),
            SettingKey::StrictMode => self.strict_mode = value == "true",
            SettingKey::DayStartMinutes => self.day_start_minutes = number(),
            SettingKey::IdleThresholdSecs => self.idle_threshold_secs = number(),
            SettingKey::ShowHudOverlay => self.show_hud_overlay = value == "true",
            SettingKey::ShowHudInFullscreen => self.show_hud_in_fullscreen = value == "true",
            SettingKey::HudPeekHotkey => self.hud_peek_hotkey = value.to_string(),
            SettingKey::AlertVolume => self.alert_volume = number(),
            SettingKey::FamilyDns | SettingKey::CaptureWindowTitles => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_shipped_values() {
        let p = Policy::default();
        assert_eq!(p.limit_cooldown_hours, 24);
        assert!(!p.strict_mode);
        assert_eq!(p.day_start_minutes, 0);
        assert_eq!(p.idle_threshold_secs, 60);
        assert!(p.show_hud_overlay);
        assert!(!p.show_hud_in_fullscreen);
        assert_eq!(p.hud_peek_hotkey, "Ctrl+Alt+T");
        assert_eq!(p.alert_volume, 80);
    }

    #[test]
    fn load_reads_stored_values_and_ignores_invalid_rows() {
        let db = Db::open_in_memory().expect("db");
        db.set_setting("day_start_minutes", "240").expect("set");
        db.set_setting("strict_mode", "true").expect("set");
        db.set_setting("alert_volume", "9000").expect("set");
        db.set_setting("limit_cooldown_hours", "banana")
            .expect("set");

        let p = Policy::load(&db);
        assert_eq!(p.day_start_minutes, 240);
        assert!(p.strict_mode);
        assert_eq!(p.alert_volume, 80, "out of range falls back to default");
        assert_eq!(
            p.limit_cooldown_hours, 24,
            "unparsable falls back to default"
        );
    }
}
