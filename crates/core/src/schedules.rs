//! Pure domain models and interval math for downtime schedules.
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

/// The local wall-clock minute of day (0..1440) and Monday-first weekday
/// (0..7) for `now`, both derived from the *same* local instant.
///
/// Downtime schedules are wall-clock windows ("22:00–07:00 on school nights"),
/// so they must not use [`DayKey`](crate::daykey::DayKey)'s weekday: the day
/// key honours the configurable day-start offset, and mixing it with a
/// wall-clock minute evaluated overnight windows against the wrong weekday
/// bit whenever the day start was not midnight.
pub fn local_minute_and_weekday(now: DateTime<Utc>, tz_offset_secs: i32) -> (u32, u32) {
    use chrono::{Datelike, Timelike};
    let local = now.naive_utc() + chrono::Duration::seconds(tz_offset_secs as i64);
    (
        local.hour() * 60 + local.minute(),
        local.weekday().num_days_from_monday(),
    )
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
    fn local_minute_and_weekday_use_one_local_instant() {
        // 2026-08-18 is a Tuesday. 01:30Z at UTC+2 is 03:30 Tuesday local.
        let now = DateTime::parse_from_rfc3339("2026-08-18T01:30:00Z")
            .expect("valid")
            .with_timezone(&Utc);
        assert_eq!(local_minute_and_weekday(now, 2 * 3600), (3 * 60 + 30, 1));
        // West of UTC the local date is still Monday.
        assert_eq!(local_minute_and_weekday(now, -5 * 3600), (20 * 60 + 30, 0));
    }

    #[test]
    fn an_overnight_window_covers_the_small_hours_regardless_of_day_start() {
        // Monday-only 22:00-07:00. At 02:00 Tuesday local it must be active:
        // the morning half belongs to Monday's window. (A 04:00 day start used
        // to make the enforcer look up Sunday's bit here.)
        let monday_night = DowntimeSchedule {
            id: 1,
            name: "Bedtime".into(),
            weekday_mask: 0b000_0001,
            start_minute: 22 * 60,
            end_minute: 7 * 60,
            enabled: true,
        };
        let now = DateTime::parse_from_rfc3339("2026-08-18T02:00:00Z")
            .expect("valid")
            .with_timezone(&Utc);
        let (minute, weekday) = local_minute_and_weekday(now, 0);
        assert!(is_schedule_active(&monday_night, minute, weekday));
    }
}
