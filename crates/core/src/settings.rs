//! The user-editable settings surface: which keys exist, which values are
//! valid, and which changes *loosen* enforcement.
//!
//! # Why this lives in `st-core`
//!
//! `SetSetting` used to write any key/value pair straight into the `settings`
//! table — the same table that stores `pin_hash`. Any local process able to
//! open the pipe could therefore replace the PIN, switch strict mode off or
//! zero the cooldown without a credential. The agent now parses every request
//! through [`SettingKey`] first: unknown keys are refused, values are range
//! checked, and [`SettingKey::is_loosening`] decides whether the change needs
//! the PIN. Keeping that decision pure keeps it unit testable.
//!
//! # Loosening rules
//!
//! | Key | Free | Needs PIN |
//! |---|---|---|
//! | `strict_mode`, `family_dns` | switching on | switching off |
//! | `limit_cooldown_hours` | raising | lowering |
//! | `day_start_minutes`, `idle_threshold_secs` | — | any change (either direction can be gamed) |
//! | `profile` (who Tether is for) | — | any change |
//! | HUD, volume, hotkey, window titles | always | — |
//!
//! A value equal to the current one is never loosening, so idempotent saves
//! from the UI do not prompt.

/// Every key `SetSetting` accepts. Anything else — notably `pin_hash` and
/// `recovery_hash` — is rejected before it reaches storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingKey {
    StrictMode,
    FamilyDns,
    LimitCooldownHours,
    DayStartMinutes,
    IdleThresholdSecs,
    ShowHudOverlay,
    ShowHudInFullscreen,
    HudPeekHotkey,
    AlertVolume,
    CaptureWindowTitles,
    /// Who Tether is set up for: `self` or `guardian`. Chosen on first run;
    /// it sets wording and defaults in the UI (docs/DESIGN_SYSTEM.md §1).
    Profile,
}

/// Values accepted for [`SettingKey::Profile`].
pub const PROFILES: [&str; 2] = ["self", "guardian"];

/// Inclusive bounds for the numeric settings.
pub const COOLDOWN_HOURS_RANGE: (i64, i64) = (0, 168);
pub const DAY_START_MINUTES_RANGE: (i64, i64) = (0, 1439);
pub const IDLE_THRESHOLD_SECS_RANGE: (i64, i64) = (5, 3600);
pub const ALERT_VOLUME_RANGE: (i64, i64) = (0, 100);
/// Longest accepted hotkey string, e.g. `Ctrl+Shift+Alt+F12`.
pub const HOTKEY_MAX_LEN: usize = 32;

impl SettingKey {
    pub const ALL: [SettingKey; 11] = [
        SettingKey::StrictMode,
        SettingKey::FamilyDns,
        SettingKey::LimitCooldownHours,
        SettingKey::DayStartMinutes,
        SettingKey::IdleThresholdSecs,
        SettingKey::ShowHudOverlay,
        SettingKey::ShowHudInFullscreen,
        SettingKey::HudPeekHotkey,
        SettingKey::AlertVolume,
        SettingKey::CaptureWindowTitles,
        SettingKey::Profile,
    ];

    /// The key as it appears on the wire (`SetSetting.key`).
    pub fn wire_name(self) -> &'static str {
        match self {
            SettingKey::StrictMode => "strict_mode",
            SettingKey::FamilyDns => "family_dns",
            SettingKey::LimitCooldownHours => "limit_cooldown_hours",
            SettingKey::DayStartMinutes => "day_start_minutes",
            SettingKey::IdleThresholdSecs => "idle_threshold_secs",
            SettingKey::ShowHudOverlay => "show_hud_overlay",
            SettingKey::ShowHudInFullscreen => "show_hud_in_fullscreen",
            SettingKey::HudPeekHotkey => "hud_peek_hotkey",
            SettingKey::AlertVolume => "alert_volume",
            SettingKey::CaptureWindowTitles => "capture_window_titles",
            SettingKey::Profile => "profile",
        }
    }

    /// The row name in the `settings` table. Identical to the wire name except
    /// for Family DNS, whose stored flag predates this enum.
    pub fn storage_key(self) -> &'static str {
        match self {
            SettingKey::FamilyDns => "family_dns_enabled",
            other => other.wire_name(),
        }
    }

    pub fn parse(wire: &str) -> Option<SettingKey> {
        SettingKey::ALL.into_iter().find(|k| k.wire_name() == wire)
    }

    /// The value assumed when the setting has never been written. Must match
    /// what the agent uses at startup.
    pub fn default_value(self) -> &'static str {
        match self {
            SettingKey::StrictMode => "false",
            SettingKey::FamilyDns => "false",
            SettingKey::LimitCooldownHours => "24",
            SettingKey::DayStartMinutes => "0",
            SettingKey::IdleThresholdSecs => "60",
            SettingKey::ShowHudOverlay => "true",
            SettingKey::ShowHudInFullscreen => "false",
            SettingKey::HudPeekHotkey => "Ctrl+Alt+T",
            SettingKey::AlertVolume => "80",
            SettingKey::CaptureWindowTitles => "false",
            SettingKey::Profile => "self",
        }
    }

    /// Validate `raw` and return its canonical stored form (trimmed numbers,
    /// lowercase booleans). The error is a human-readable reason.
    pub fn normalize(self, raw: &str) -> Result<String, String> {
        let raw = raw.trim();
        match self {
            SettingKey::StrictMode
            | SettingKey::FamilyDns
            | SettingKey::ShowHudOverlay
            | SettingKey::ShowHudInFullscreen
            | SettingKey::CaptureWindowTitles => match raw {
                "true" | "false" => Ok(raw.to_string()),
                _ => Err(format!("{} must be true or false", self.wire_name())),
            },
            SettingKey::LimitCooldownHours => in_range(self, raw, COOLDOWN_HOURS_RANGE),
            SettingKey::DayStartMinutes => in_range(self, raw, DAY_START_MINUTES_RANGE),
            SettingKey::IdleThresholdSecs => in_range(self, raw, IDLE_THRESHOLD_SECS_RANGE),
            SettingKey::AlertVolume => in_range(self, raw, ALERT_VOLUME_RANGE),
            SettingKey::Profile => {
                if PROFILES.contains(&raw) {
                    Ok(raw.to_string())
                } else {
                    Err("profile must be self or guardian".to_string())
                }
            }
            SettingKey::HudPeekHotkey => {
                if raw.is_empty() || raw.len() > HOTKEY_MAX_LEN || raw.chars().any(char::is_control)
                {
                    Err(format!(
                        "hud_peek_hotkey must be 1-{HOTKEY_MAX_LEN} printable characters"
                    ))
                } else {
                    Ok(raw.to_string())
                }
            }
        }
    }

    /// Whether moving from `current` (the stored value, or `None` when never
    /// written) to the already-normalized `new` value weakens enforcement.
    pub fn is_loosening(self, current: Option<&str>, new: &str) -> bool {
        let current = current.unwrap_or(self.default_value()).trim();
        if current == new {
            return false;
        }
        match self {
            // Switching protection off is loosening; on is not.
            SettingKey::StrictMode | SettingKey::FamilyDns => current == "true" && new == "false",
            // A shorter cooldown lets loosened limits land sooner.
            SettingKey::LimitCooldownHours => match (current.parse::<i64>(), new.parse::<i64>()) {
                (Ok(old), Ok(new)) => new < old,
                // Unparsable stored value: be conservative.
                _ => true,
            },
            // Moving the day boundary can re-open an exhausted budget, and a
            // lower idle threshold stops accruing usage sooner. Either
            // direction can be gamed, so any change is gated.
            SettingKey::DayStartMinutes | SettingKey::IdleThresholdSecs => true,
            // Switching between "me" and "someone I look after" changes how
            // strict the UI's defaults are; only whoever holds the PIN may.
            SettingKey::Profile => true,
            SettingKey::ShowHudOverlay
            | SettingKey::ShowHudInFullscreen
            | SettingKey::HudPeekHotkey
            | SettingKey::AlertVolume
            | SettingKey::CaptureWindowTitles => false,
        }
    }
}

fn in_range(key: SettingKey, raw: &str, (min, max): (i64, i64)) -> Result<String, String> {
    match raw.parse::<i64>() {
        Ok(v) if (min..=max).contains(&v) => Ok(v.to_string()),
        _ => Err(format!(
            "{} must be a whole number from {min} to {max}",
            key.wire_name()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_round_trips_through_its_wire_name() {
        for key in SettingKey::ALL {
            assert_eq!(SettingKey::parse(key.wire_name()), Some(key));
        }
    }

    #[test]
    fn vault_and_unknown_keys_are_rejected() {
        for key in [
            "pin_hash",
            "recovery_hash",
            "family_dns_enabled",
            "",
            "STRICT_MODE",
        ] {
            assert_eq!(SettingKey::parse(key), None, "{key} must not parse");
        }
    }

    #[test]
    fn family_dns_is_stored_under_its_legacy_row_name() {
        assert_eq!(SettingKey::FamilyDns.storage_key(), "family_dns_enabled");
        assert_eq!(SettingKey::StrictMode.storage_key(), "strict_mode");
    }

    #[test]
    fn defaults_are_themselves_valid() {
        for key in SettingKey::ALL {
            assert_eq!(
                key.normalize(key.default_value()).as_deref(),
                Ok(key.default_value()),
                "{key:?}"
            );
        }
    }

    #[test]
    fn numbers_are_range_checked_and_canonicalised() {
        assert_eq!(
            SettingKey::LimitCooldownHours.normalize(" 48 ").as_deref(),
            Ok("48")
        );
        assert!(SettingKey::LimitCooldownHours.normalize("169").is_err());
        assert!(SettingKey::LimitCooldownHours.normalize("-1").is_err());
        assert!(SettingKey::DayStartMinutes.normalize("1440").is_err());
        assert!(SettingKey::IdleThresholdSecs.normalize("4").is_err());
        assert!(SettingKey::AlertVolume.normalize("101").is_err());
        assert!(SettingKey::AlertVolume.normalize("abc").is_err());
    }

    #[test]
    fn booleans_accept_only_canonical_spellings() {
        assert_eq!(
            SettingKey::StrictMode.normalize("true").as_deref(),
            Ok("true")
        );
        assert!(SettingKey::StrictMode.normalize("yes").is_err());
        assert!(SettingKey::StrictMode.normalize("1").is_err());
    }

    #[test]
    fn hotkeys_must_be_short_and_printable() {
        assert!(SettingKey::HudPeekHotkey.normalize("Ctrl+Alt+P").is_ok());
        assert!(SettingKey::HudPeekHotkey.normalize("").is_err());
        assert!(SettingKey::HudPeekHotkey
            .normalize(&"x".repeat(33))
            .is_err());
        assert!(SettingKey::HudPeekHotkey.normalize("Ctrl+\u{7}").is_err());
    }

    #[test]
    fn switching_protection_off_is_loosening_and_on_is_not() {
        for key in [SettingKey::StrictMode, SettingKey::FamilyDns] {
            assert!(key.is_loosening(Some("true"), "false"));
            assert!(!key.is_loosening(Some("false"), "true"));
            // Never written: defaults to off, so "off" is a no-op.
            assert!(!key.is_loosening(None, "false"));
        }
    }

    #[test]
    fn lowering_the_cooldown_is_loosening_and_raising_is_not() {
        let key = SettingKey::LimitCooldownHours;
        assert!(key.is_loosening(Some("24"), "0"));
        assert!(!key.is_loosening(Some("24"), "48"));
        assert!(key.is_loosening(None, "12"), "default is 24");
        assert!(
            key.is_loosening(Some("garbage"), "24"),
            "unparsable is conservative"
        );
    }

    #[test]
    fn any_change_to_day_start_or_idle_threshold_is_loosening() {
        for key in [SettingKey::DayStartMinutes, SettingKey::IdleThresholdSecs] {
            let current = key.default_value();
            assert!(!key.is_loosening(Some(current), current));
            assert!(key.is_loosening(Some(current), "300"));
        }
    }

    #[test]
    fn profile_accepts_two_values_and_any_change_needs_the_pin() {
        let key = SettingKey::Profile;
        assert_eq!(key.normalize(" guardian ").as_deref(), Ok("guardian"));
        assert!(key.normalize("parent").is_err());
        assert!(key.is_loosening(None, "guardian"), "default is self");
        assert!(key.is_loosening(Some("guardian"), "self"));
        assert!(!key.is_loosening(Some("self"), "self"));
    }

    #[test]
    fn cosmetic_settings_never_need_a_pin() {
        for key in [
            SettingKey::ShowHudOverlay,
            SettingKey::ShowHudInFullscreen,
            SettingKey::AlertVolume,
            SettingKey::HudPeekHotkey,
            SettingKey::CaptureWindowTitles,
        ] {
            assert!(!key.is_loosening(None, "anything"));
        }
    }
}
