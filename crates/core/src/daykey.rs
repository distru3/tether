//! Which calendar day does a moment of usage belong to?
//!
//! This is deceptively hard and it is where screen-time apps most often get
//! things wrong, so it lives in one small, heavily tested place.
//!
//! Three things are in play:
//!
//! * **Timezone.** Usage is bucketed by the user's *local* day, not UTC.
//! * **Day start offset.** A configurable offset (default `0` = midnight). Set
//!   it to `240` (04:00) and browsing at 01:30 still counts against the
//!   previous day, which is what night owls expect.
//! * **Variable day length.** DST means local days can be 23 or 25 hours long.
//!   Because the offset is re-read from the [`Clock`](crate::clock::Clock) on
//!   every call rather than cached, a DST transition simply shifts subsequent
//!   buckets; no day is skipped or double-counted.

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A local calendar day encoded as `YYYYMMDD`.
///
/// Stored as an integer because it is compact, sorts correctly, indexes well in
/// SQLite, and is readable when eyeballing the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DayKey(pub i32);

impl DayKey {
    /// Bucket a UTC instant into a local day.
    ///
    /// * `tz_offset_secs` — seconds to add to UTC for local time.
    /// * `day_start_minutes` — minutes after local midnight at which the day
    ///   rolls over. `0` for midnight, `240` for 04:00.
    pub fn from_utc(now: DateTime<Utc>, tz_offset_secs: i32, day_start_minutes: i64) -> Self {
        let offset = fixed_offset(tz_offset_secs);
        let local = now.with_timezone(&offset).naive_local();
        let shifted = local - minutes(day_start_minutes);
        Self::from_date(shifted.date())
    }

    pub fn from_date(date: NaiveDate) -> Self {
        DayKey(date.year() * 10_000 + date.month() as i32 * 100 + date.day() as i32)
    }

    pub fn to_date(self) -> Option<NaiveDate> {
        let y = self.0 / 10_000;
        let m = (self.0 / 100 % 100) as u32;
        let d = (self.0 % 100) as u32;
        NaiveDate::from_ymd_opt(y, m, d)
    }

    /// The instant, in UTC, at which this day ends and budgets reset.
    ///
    /// This is what "blocked until the day ends" resolves to, and what gets
    /// written into `block_state.expires_at`.
    pub fn end_utc(self, tz_offset_secs: i32, day_start_minutes: i64) -> Option<DateTime<Utc>> {
        let next = self.to_date()?.succ_opt()?;
        let boundary_local = next.and_hms_opt(0, 0, 0)? + minutes(day_start_minutes);
        // With a fixed offset the mapping is exact: utc = local - offset.
        let utc_naive = boundary_local - Duration::seconds(i64::from(tz_offset_secs));
        Some(DateTime::from_naive_utc_and_offset(utc_naive, Utc))
    }

    pub fn succ(self) -> Option<Self> {
        Some(Self::from_date(self.to_date()?.succ_opt()?))
    }

    pub fn pred(self) -> Option<Self> {
        Some(Self::from_date(self.to_date()?.pred_opt()?))
    }

    /// Monday = 0 … Sunday = 6. Used to pick per-weekday limit overrides.
    pub fn weekday_index(self) -> Option<usize> {
        Some(self.to_date()?.weekday().num_days_from_monday() as usize)
    }
}

impl fmt::Display for DayKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_date() {
            Some(d) => write!(f, "{d}"),
            None => write!(f, "invalid-daykey({})", self.0),
        }
    }
}

fn fixed_offset(secs: i32) -> FixedOffset {
    FixedOffset::east_opt(secs)
        .unwrap_or_else(|| FixedOffset::east_opt(0).expect("zero offset is valid"))
}

fn minutes(m: i64) -> Duration {
    Duration::try_minutes(m).unwrap_or_else(Duration::zero)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .expect("valid rfc3339")
            .with_timezone(&Utc)
    }

    #[test]
    fn utc_midnight_rollover() {
        assert_eq!(DayKey::from_utc(at("2026-08-20T23:59:59Z"), 0, 0), DayKey(20260820));
        assert_eq!(DayKey::from_utc(at("2026-08-21T00:00:00Z"), 0, 0), DayKey(20260821));
    }

    #[test]
    fn local_timezone_shifts_the_bucket() {
        // 23:30 UTC is already the 21st in UTC+2.
        let utc_plus_two = 2 * 3600;
        assert_eq!(
            DayKey::from_utc(at("2026-08-20T23:30:00Z"), utc_plus_two, 0),
            DayKey(20260821)
        );
        // ...but still the 20th in UTC-5.
        let utc_minus_five = -5 * 3600;
        assert_eq!(
            DayKey::from_utc(at("2026-08-20T23:30:00Z"), utc_minus_five, 0),
            DayKey(20260820)
        );
    }

    #[test]
    fn four_am_day_start_keeps_late_nights_on_the_previous_day() {
        let day_start = 240; // 04:00
        // 01:30 local on the 21st still belongs to the 20th.
        assert_eq!(
            DayKey::from_utc(at("2026-08-21T01:30:00Z"), 0, day_start),
            DayKey(20260820)
        );
        // 04:00 local flips over.
        assert_eq!(
            DayKey::from_utc(at("2026-08-21T04:00:00Z"), 0, day_start),
            DayKey(20260821)
        );
    }

    #[test]
    fn day_end_is_the_next_rollover_instant() {
        let end = DayKey(20260820)
            .end_utc(0, 0)
            .expect("valid boundary");
        assert_eq!(end, at("2026-08-21T00:00:00Z"));

        // With a 04:00 day start in UTC+2, the 20th ends at 02:00Z on the 21st.
        let end = DayKey(20260820)
            .end_utc(2 * 3600, 240)
            .expect("valid boundary");
        assert_eq!(end, at("2026-08-21T02:00:00Z"));
    }

    #[test]
    fn month_and_year_boundaries_roll_correctly() {
        assert_eq!(DayKey(20260831).succ(), Some(DayKey(20260901)));
        assert_eq!(DayKey(20261231).succ(), Some(DayKey(20270101)));
        assert_eq!(DayKey(20260301).pred(), Some(DayKey(20260228)));
        // 2028 is a leap year.
        assert_eq!(DayKey(20280301).pred(), Some(DayKey(20280229)));
    }

    #[test]
    fn dst_transition_does_not_skip_a_day() {
        // Europe/Berlin springs forward 2026-03-29 02:00 local (+1 -> +2).
        let before = DayKey::from_utc(at("2026-03-28T23:30:00Z"), 3600, 0);
        let after = DayKey::from_utc(at("2026-03-29T12:00:00Z"), 2 * 3600, 0);
        assert_eq!(before, DayKey(20260329));
        assert_eq!(after, DayKey(20260329));
        assert_eq!(before.succ(), Some(DayKey(20260330)));
    }

    #[test]
    fn weekday_index_is_monday_zero() {
        // 2026-08-20 is a Thursday.
        assert_eq!(DayKey(20260820).weekday_index(), Some(3));
    }

    #[test]
    fn invalid_keys_round_trip_to_none() {
        assert_eq!(DayKey(20260230).to_date(), None);
        assert_eq!(DayKey(0).to_date(), None);
    }
}
