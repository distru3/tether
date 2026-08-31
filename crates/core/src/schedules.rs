//! Pure domain models and interval math for downtime schedules and focus sessions.
//!
//! # Invariants
//!
//! - **No OS APIs, no wall clock, no SQL.** All time arguments (`now`,
//!   `local_minute`, `weekday_index`) are injected.
//! - **Monday-first weekday indexing**: 0 = Monday, 1 = Tuesday, ..., 6 = Sunday.
//! - **Overnight wrap-around**: when `start_minute > end_minute` (e.g. 22:00 to
//!   07:00), the morning portion (`00:00..end_minute`) is evaluated against the
//!   *previous* day's active weekday mask bit.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A recurring downtime schedule window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DowntimeSchedule {
    pub id: i64,
    pub name: String,
    /// Bitmask: bit 0 = Monday, bit 1 = Tuesday, ..., bit 6 = Sunday.
    pub weekday_mask: u8,
    /// Minute of local day (0..=1439).
    pub start_minute: u32,
    /// Minute of local day (0..=1439).
    pub end_minute: u32,
    pub enabled: bool,
}

/// Check if `weekday_mask` has the bit for `weekday_index` set.
/// `weekday_index`: 0 = Monday .. 6 = Sunday.
pub fn weekday_mask_contains(mask: u8, weekday_index: u32) -> bool {
    if weekday_index > 6 {
        return false;
    }
    (mask & (1 << weekday_index)) != 0
}

/// Compute the previous weekday index (Monday=0 -> Sunday=6).
fn previous_weekday(weekday_index: u32) -> u32 {
    (weekday_index + 6) % 7
}

/// Check if a downtime schedule is currently in force for a given local minute and weekday.
pub fn is_schedule_active(
    schedule: &DowntimeSchedule,
    local_minute: u32,
    weekday_index: u32,
) -> bool {
    if !schedule.enabled || weekday_index > 6 || local_minute >= 1440 {
        return false;
    }

    if schedule.start_minute <= schedule.end_minute {
        // Same-day window (e.g. 08:00 to 15:00)
        weekday_mask_contains(schedule.weekday_mask, weekday_index)
            && local_minute >= schedule.start_minute
            && local_minute < schedule.end_minute
    } else {
        // Overnight window (e.g. 22:00 to 07:00)
        if local_minute >= schedule.start_minute {
            // Evening half: check today's weekday bit
            weekday_mask_contains(schedule.weekday_mask, weekday_index)
        } else if local_minute < schedule.end_minute {
            // Morning half: belongs to the schedule that started yesterday evening
            weekday_mask_contains(schedule.weekday_mask, previous_weekday(weekday_index))
        } else {
            false
        }
    }
}

/// Check if ANY enabled schedule in `schedules` is currently active.
pub fn is_any_downtime_active(
    schedules: &[DowntimeSchedule],
    local_minute: u32,
    weekday_index: u32,
) -> bool {
    schedules
        .iter()
        .any(|s| is_schedule_active(s, local_minute, weekday_index))
}

/// An active, timed distraction-free focus session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusSession {
    pub name: Option<String>,
    pub started_at_utc: DateTime<Utc>,
    pub duration_minutes: u32,
    pub expires_utc: DateTime<Utc>,
}

impl FocusSession {
    pub fn new(name: Option<String>, started_at_utc: DateTime<Utc>, duration_minutes: u32) -> Self {
        let duration_secs = (duration_minutes as i64) * 60;
        let expires_utc = started_at_utc + chrono::Duration::seconds(duration_secs);
        Self {
            name,
            started_at_utc,
            duration_minutes,
            expires_utc,
        }
    }

    /// Check if the focus session is currently running.
    pub fn is_active(&self, now: DateTime<Utc>) -> bool {
        now >= self.started_at_utc && now < self.expires_utc
    }

    /// Remaining seconds in the session, or 0 if expired.
    pub fn remaining_seconds(&self, now: DateTime<Utc>) -> i64 {
        if now >= self.expires_utc {
            0
        } else {
            (self.expires_utc - now).num_seconds().max(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekday_mask_contains_checks_bits_correctly() {
        // Mon (0), Wed (2), Fri (4): 1 | 4 | 16 = 21
        let mask = 0b00010101;
        assert!(weekday_mask_contains(mask, 0)); // Mon
        assert!(!weekday_mask_contains(mask, 1)); // Tue
        assert!(weekday_mask_contains(mask, 2)); // Wed
        assert!(!weekday_mask_contains(mask, 3)); // Thu
        assert!(weekday_mask_contains(mask, 4)); // Fri
        assert!(!weekday_mask_contains(mask, 5)); // Sat
        assert!(!weekday_mask_contains(mask, 6)); // Sun
        assert!(!weekday_mask_contains(mask, 7)); // Out of bounds
    }

    #[test]
    fn same_day_schedule_evaluates_correctly() {
        // School hours: Mon-Fri, 08:00 (480) to 15:00 (900)
        let mask = 0b00011111; // Mon-Fri
        let schedule = DowntimeSchedule {
            id: 1,
            name: "School".into(),
            weekday_mask: mask,
            start_minute: 480,
            end_minute: 900,
            enabled: true,
        };

        // Tuesday (1) at 10:00 (600) -> active
        assert!(is_schedule_active(&schedule, 600, 1));
        // Tuesday (1) at 07:59 (479) -> inactive
        assert!(!is_schedule_active(&schedule, 479, 1));
        // Tuesday (1) at 15:00 (900) -> inactive (exclusive end)
        assert!(!is_schedule_active(&schedule, 900, 1));
        // Saturday (5) at 10:00 (600) -> inactive (not on mask)
        assert!(!is_schedule_active(&schedule, 600, 5));
    }

    #[test]
    fn overnight_wrap_schedule_evaluates_correctly() {
        // Bedtime: Sun-Thu nights, 22:00 (1320) to 07:00 (420)
        // Sun = bit 6, Mon = 0, Tue = 1, Wed = 2, Thu = 3
        let mask = (1 << 6) | (1 << 0) | (1 << 1) | (1 << 2) | (1 << 3);
        let schedule = DowntimeSchedule {
            id: 2,
            name: "Bedtime".into(),
            weekday_mask: mask,
            start_minute: 1320,
            end_minute: 420,
            enabled: true,
        };

        // Sunday evening (6) at 23:00 (1380) -> active
        assert!(is_schedule_active(&schedule, 1380, 6));
        // Monday morning (0) at 05:00 (300) -> active (belongs to Sunday evening start)
        assert!(is_schedule_active(&schedule, 300, 0));
        // Monday morning (0) at 07:00 (420) -> inactive
        assert!(!is_schedule_active(&schedule, 420, 0));
        // Friday evening (4) at 23:00 (1380) -> inactive (Friday night not in mask)
        assert!(!is_schedule_active(&schedule, 1380, 4));
        // Saturday morning (5) at 05:00 (300) -> inactive (Friday night was not in mask)
        assert!(!is_schedule_active(&schedule, 300, 5));
    }

    #[test]
    fn disabled_schedule_never_activates() {
        let schedule = DowntimeSchedule {
            id: 3,
            name: "Disabled".into(),
            weekday_mask: 0b01111111,
            start_minute: 0,
            end_minute: 1439,
            enabled: false,
        };
        assert!(!is_schedule_active(&schedule, 500, 0));
    }

    #[test]
    fn focus_session_countdown_and_expiry() {
        let start = DateTime::parse_from_rfc3339("2026-08-29T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let session = FocusSession::new(Some("Pomodoro".into()), start, 25);

        assert_eq!(session.duration_minutes, 25);
        assert_eq!(
            session.expires_utc.to_rfc3339(),
            "2026-08-29T10:25:00+00:00"
        );

        let mid = start + chrono::Duration::minutes(10);
        assert!(session.is_active(mid));
        assert_eq!(session.remaining_seconds(mid), 15 * 60);

        let after = start + chrono::Duration::minutes(26);
        assert!(!session.is_active(after));
        assert_eq!(session.remaining_seconds(after), 0);
    }
}
