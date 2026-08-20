//! Turns a stream of 1 Hz samples into usage intervals.
//!
//! Polling produces one observation per second; storing them raw would mean
//! 86,400 rows per day per user. Instead, consecutive identical samples are
//! collapsed into a single interval, so an uninterrupted two-hour session is one
//! row.
//!
//! Deliberately free of any OS or database dependency: the whole thing is driven
//! by values, which is why the awkward cases (midnight rollover mid-session,
//! going idle, the clock being tampered with) are unit-testable without a VM.

use chrono::{DateTime, Utc};
use st_core::clock::ClockVerdict;
use st_core::daykey::DayKey;
use st_core::model::AppKey;
use st_core::platform::{ActiveWindow, IdleState};

/// A completed span of usage, ready to be written to storage.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingInterval {
    pub key: AppKey,
    pub display_name: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub day: DayKey,
}

impl PendingInterval {
    pub fn duration_secs(&self) -> i64 {
        (self.end - self.start).num_seconds().max(0)
    }
}

struct OpenInterval {
    key: AppKey,
    display_name: String,
    start: DateTime<Utc>,
    /// Last instant we positively observed this app in the foreground. Used as
    /// the interval end so that a discarded final sample never inflates usage.
    last_seen: DateTime<Utc>,
    day: DayKey,
}

pub struct Sampler {
    open: Option<OpenInterval>,
    day_start_minutes: i64,
}

impl Sampler {
    pub fn new(day_start_minutes: i64) -> Self {
        Self {
            open: None,
            day_start_minutes,
        }
    }

    pub fn day_start_minutes(&self) -> i64 {
        self.day_start_minutes
    }

    /// Feed one observation. Returns any intervals that just closed.
    ///
    /// Returns a `Vec` because a single sample can close an interval *and*
    /// nothing else, or close one and open another; the caller should not have
    /// to care which.
    pub fn observe(
        &mut self,
        now: DateTime<Utc>,
        tz_offset_secs: i32,
        window: Option<&ActiveWindow>,
        idle: IdleState,
        verdict: ClockVerdict,
    ) -> Vec<PendingInterval> {
        // A wall-clock jump means the elapsed time is not trustworthy: either
        // the user moved the clock, or the machine was asleep. Close the open
        // interval at its last good observation and credit nothing for the gap.
        if !matches!(verdict, ClockVerdict::Normal { .. }) {
            return self.close().into_iter().collect();
        }

        // Idle, locked, or nothing focused: usage stops accruing.
        let Some(window) = window.filter(|_| idle.is_countable()) else {
            return self.close().into_iter().collect();
        };

        let day = DayKey::from_utc(now, tz_offset_secs, self.day_start_minutes);

        match &mut self.open {
            // Same app, same day: extend.
            Some(open) if open.key == window.key && open.day == day => {
                open.last_seen = now;
                Vec::new()
            }
            // Same app, but the day rolled over while it was in the foreground.
            // Split so each calendar day is charged for its own share; leaving
            // this out is how an all-night session lands entirely on one day.
            Some(_) => {
                let mut closed: Vec<PendingInterval> = self.close().into_iter().collect();
                self.open_new(window, now, day);
                closed.reserve(0);
                closed
            }
            None => {
                self.open_new(window, now, day);
                Vec::new()
            }
        }
    }

    /// Close any open interval, e.g. on shutdown. Must be called before exit or
    /// the final session is lost.
    pub fn flush(&mut self) -> Option<PendingInterval> {
        self.close()
    }

    fn open_new(&mut self, window: &ActiveWindow, now: DateTime<Utc>, day: DayKey) {
        self.open = Some(OpenInterval {
            key: window.key.clone(),
            display_name: window.display_name.clone(),
            start: now,
            last_seen: now,
            day,
        });
    }

    fn close(&mut self) -> Option<PendingInterval> {
        let open = self.open.take()?;
        // Zero-length intervals happen whenever an app is focused for less than
        // one poll period. They carry no information and would clutter the
        // database, so drop them.
        if open.last_seen <= open.start {
            return None;
        }
        Some(PendingInterval {
            key: open.key,
            display_name: open.display_name,
            start: open.start,
            end: open.last_seen,
            day: open.day,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .expect("valid rfc3339")
            .with_timezone(&Utc)
    }

    fn window(path: &str) -> ActiveWindow {
        ActiveWindow {
            key: AppKey::windows_exe(path),
            pid: 1234,
            display_name: "Test".into(),
            title: None,
        }
    }

    const NORMAL: ClockVerdict = ClockVerdict::Normal {
        elapsed: std::time::Duration::from_secs(1),
    };

    #[test]
    fn consecutive_identical_samples_collapse_into_one_interval() {
        let mut s = Sampler::new(0);
        let w = window("C:\\a.exe");
        let start = at("2026-08-20T10:00:00Z");

        for i in 0..60 {
            let closed = s.observe(start + Duration::seconds(i), 0, Some(&w), IdleState::Active, NORMAL);
            assert!(closed.is_empty(), "nothing should close mid-session");
        }

        let interval = s.flush().expect("one interval");
        assert_eq!(interval.duration_secs(), 59);
        assert_eq!(interval.day, DayKey(20260820));
    }

    #[test]
    fn switching_apps_closes_the_previous_interval() {
        let mut s = Sampler::new(0);
        let a = window("C:\\a.exe");
        let b = window("C:\\b.exe");
        let t = at("2026-08-20T10:00:00Z");

        s.observe(t, 0, Some(&a), IdleState::Active, NORMAL);
        s.observe(t + Duration::seconds(30), 0, Some(&a), IdleState::Active, NORMAL);
        let closed = s.observe(t + Duration::seconds(31), 0, Some(&b), IdleState::Active, NORMAL);

        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].key, a.key);
        assert_eq!(closed[0].duration_secs(), 30);
    }

    #[test]
    fn going_idle_stops_accrual() {
        let mut s = Sampler::new(0);
        let w = window("C:\\a.exe");
        let t = at("2026-08-20T10:00:00Z");

        s.observe(t, 0, Some(&w), IdleState::Active, NORMAL);
        s.observe(t + Duration::seconds(10), 0, Some(&w), IdleState::Active, NORMAL);
        let closed = s.observe(
            t + Duration::seconds(11),
            0,
            Some(&w),
            IdleState::Idle { for_secs: 120 },
            NORMAL,
        );

        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].duration_secs(), 10, "idle time must not be credited");

        // Staying idle emits nothing further.
        assert!(s
            .observe(
                t + Duration::seconds(600),
                0,
                Some(&w),
                IdleState::Idle { for_secs: 700 },
                NORMAL
            )
            .is_empty());
    }

    #[test]
    fn a_locked_session_stops_accrual() {
        let mut s = Sampler::new(0);
        let w = window("C:\\a.exe");
        let t = at("2026-08-20T10:00:00Z");

        s.observe(t, 0, Some(&w), IdleState::Active, NORMAL);
        s.observe(t + Duration::seconds(5), 0, Some(&w), IdleState::Active, NORMAL);
        let closed = s.observe(t + Duration::seconds(6), 0, Some(&w), IdleState::Locked, NORMAL);

        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].duration_secs(), 5);
    }

    #[test]
    fn an_all_night_session_is_split_at_the_day_boundary() {
        let mut s = Sampler::new(0);
        let w = window("C:\\a.exe");

        s.observe(at("2026-08-20T23:59:00Z"), 0, Some(&w), IdleState::Active, NORMAL);
        s.observe(at("2026-08-20T23:59:59Z"), 0, Some(&w), IdleState::Active, NORMAL);
        let closed = s.observe(at("2026-08-21T00:00:00Z"), 0, Some(&w), IdleState::Active, NORMAL);

        assert_eq!(closed.len(), 1, "the previous day must be closed off");
        assert_eq!(closed[0].day, DayKey(20260820));
        assert_eq!(closed[0].duration_secs(), 59);

        // Usage continues on the new day.
        s.observe(at("2026-08-21T00:01:00Z"), 0, Some(&w), IdleState::Active, NORMAL);
        let next = s.flush().expect("second interval");
        assert_eq!(next.day, DayKey(20260821));
    }

    #[test]
    fn a_four_am_day_start_moves_the_split() {
        let mut s = Sampler::new(240);
        let w = window("C:\\a.exe");

        // 01:00 still belongs to the 20th.
        s.observe(at("2026-08-21T01:00:00Z"), 0, Some(&w), IdleState::Active, NORMAL);
        s.observe(at("2026-08-21T01:30:00Z"), 0, Some(&w), IdleState::Active, NORMAL);
        assert!(
            s.observe(at("2026-08-21T02:00:00Z"), 0, Some(&w), IdleState::Active, NORMAL)
                .is_empty(),
            "midnight is not the boundary when day_start is 04:00"
        );

        let closed = s.observe(at("2026-08-21T04:00:00Z"), 0, Some(&w), IdleState::Active, NORMAL);
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].day, DayKey(20260820));
    }

    #[test]
    fn a_clock_jump_discards_the_open_interval_rather_than_crediting_it() {
        let mut s = Sampler::new(0);
        let w = window("C:\\a.exe");
        let t = at("2026-08-20T10:00:00Z");

        s.observe(t, 0, Some(&w), IdleState::Active, NORMAL);
        s.observe(t + Duration::seconds(20), 0, Some(&w), IdleState::Active, NORMAL);

        let closed = s.observe(
            t + Duration::seconds(21),
            0,
            Some(&w),
            IdleState::Active,
            ClockVerdict::Backward { by: Duration::hours(1) },
        );

        // Credited only up to the last trustworthy observation.
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].duration_secs(), 20);
        assert!(s.flush().is_none(), "nothing should remain open");
    }

    #[test]
    fn a_sleep_gap_is_not_credited() {
        let mut s = Sampler::new(0);
        let w = window("C:\\a.exe");
        let t = at("2026-08-20T10:00:00Z");

        s.observe(t, 0, Some(&w), IdleState::Active, NORMAL);
        s.observe(t + Duration::seconds(15), 0, Some(&w), IdleState::Active, NORMAL);

        // Laptop lid closed for eight hours.
        let closed = s.observe(
            t + Duration::hours(8),
            0,
            Some(&w),
            IdleState::Active,
            ClockVerdict::Forward { by: Duration::hours(8) },
        );
        assert_eq!(closed[0].duration_secs(), 15, "sleeping is not screen time");
    }

    #[test]
    fn losing_focus_to_the_desktop_closes_the_interval() {
        let mut s = Sampler::new(0);
        let w = window("C:\\a.exe");
        let t = at("2026-08-20T10:00:00Z");

        s.observe(t, 0, Some(&w), IdleState::Active, NORMAL);
        s.observe(t + Duration::seconds(9), 0, Some(&w), IdleState::Active, NORMAL);
        let closed = s.observe(t + Duration::seconds(10), 0, None, IdleState::Active, NORMAL);

        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].duration_secs(), 9);
    }

    #[test]
    fn a_single_sample_produces_no_interval() {
        let mut s = Sampler::new(0);
        let w = window("C:\\a.exe");
        s.observe(at("2026-08-20T10:00:00Z"), 0, Some(&w), IdleState::Active, NORMAL);
        assert!(s.flush().is_none(), "zero-length intervals are noise");
    }

    #[test]
    fn timezone_change_mid_session_rebuckets_without_losing_time() {
        let mut s = Sampler::new(0);
        let w = window("C:\\a.exe");

        // 23:30 UTC, user in UTC+0 -> the 20th.
        s.observe(at("2026-08-20T23:30:00Z"), 0, Some(&w), IdleState::Active, NORMAL);
        s.observe(at("2026-08-20T23:40:00Z"), 0, Some(&w), IdleState::Active, NORMAL);

        // User flies east; offset becomes UTC+2, so local time is now the 21st.
        let closed = s.observe(
            at("2026-08-20T23:41:00Z"),
            2 * 3600,
            Some(&w),
            IdleState::Active,
            NORMAL,
        );

        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].day, DayKey(20260820));
        assert_eq!(closed[0].duration_secs(), 600);
    }
}
