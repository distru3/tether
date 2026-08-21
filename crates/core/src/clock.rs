//! Time access, abstracted so that every time-dependent rule is testable.
//!
//! Two independent notions of time are tracked deliberately:
//!
//! * **Wall time** ([`Clock::now_utc`]) — what the user's clock says. Needed to
//!   decide which calendar day usage belongs to. Attacker-controlled.
//! * **Monotonic time** ([`Clock::monotonic`]) — cannot be moved backwards by
//!   changing the system clock. Used to measure elapsed durations.
//!
//! Comparing the two is how [`ClockGuard`] notices the clock being wound back
//! to escape a limit, and how it notices a suspend/resume gap.

use std::sync::Mutex;
use std::time::{Duration as StdDuration, Instant};

use chrono::{DateTime, Duration, FixedOffset, Utc};

/// Source of truth for time. Inject a [`TestClock`] in unit tests.
pub trait Clock: Send + Sync {
    /// Current wall-clock instant in UTC. May jump in either direction.
    fn now_utc(&self) -> DateTime<Utc>;

    /// Monotonically non-decreasing duration since an arbitrary fixed origin.
    fn monotonic(&self) -> StdDuration;

    /// Seconds to add to UTC to obtain the user's local time.
    ///
    /// Re-read on every call rather than cached, because the user can change
    /// timezone (or cross a DST boundary) while the agent is running.
    fn local_offset_seconds(&self) -> i32;

    /// The user's current local UTC offset as a `chrono` offset.
    fn local_offset(&self) -> FixedOffset {
        FixedOffset::east_opt(self.local_offset_seconds())
            .unwrap_or_else(|| FixedOffset::east_opt(0).expect("zero offset is valid"))
    }
}

/// Production clock backed by the operating system.
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn monotonic(&self) -> StdDuration {
        self.origin.elapsed()
    }

    fn local_offset_seconds(&self) -> i32 {
        chrono::Local::now().offset().local_minus_utc()
    }
}

/// Manually driven clock for tests.
pub struct TestClock {
    inner: Mutex<TestClockState>,
}

struct TestClockState {
    wall: DateTime<Utc>,
    mono: StdDuration,
    offset_seconds: i32,
}

impl TestClock {
    pub fn new(wall: DateTime<Utc>, offset_seconds: i32) -> Self {
        Self {
            inner: Mutex::new(TestClockState {
                wall,
                mono: StdDuration::ZERO,
                offset_seconds,
            }),
        }
    }

    /// Advance both wall and monotonic time by the same amount: normal passage
    /// of time.
    pub fn advance(&self, by: StdDuration) {
        let mut s = self.inner.lock().expect("test clock poisoned");
        s.mono += by;
        s.wall += Duration::try_seconds(by.as_secs() as i64).expect("duration in range");
    }

    /// Move wall time only, leaving monotonic time untouched: this is what a
    /// user fiddling with the system clock looks like.
    pub fn skew_wall(&self, by: Duration) {
        let mut s = self.inner.lock().expect("test clock poisoned");
        s.wall += by;
    }

    /// Advance monotonic time only: simulates a stalled or lying wall clock.
    pub fn advance_monotonic(&self, by: StdDuration) {
        let mut s = self.inner.lock().expect("test clock poisoned");
        s.mono += by;
    }

    pub fn set_offset_seconds(&self, offset_seconds: i32) {
        self.inner
            .lock()
            .expect("test clock poisoned")
            .offset_seconds = offset_seconds;
    }
}

impl Clock for TestClock {
    fn now_utc(&self) -> DateTime<Utc> {
        self.inner.lock().expect("test clock poisoned").wall
    }

    fn monotonic(&self) -> StdDuration {
        self.inner.lock().expect("test clock poisoned").mono
    }

    fn local_offset_seconds(&self) -> i32 {
        self.inner
            .lock()
            .expect("test clock poisoned")
            .offset_seconds
    }
}

/// What happened to the clock between two [`ClockGuard::check`] calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockVerdict {
    /// Wall and monotonic time agree. `elapsed` is safe to credit as usage.
    Normal { elapsed: StdDuration },
    /// Wall time moved backwards relative to monotonic time.
    ///
    /// Treat as tampering: do not credit the interval, keep active blocks in
    /// place, and write an audit entry.
    Backward { by: Duration },
    /// Wall time ran ahead of monotonic time.
    ///
    /// Usually a suspend/resume or hibernate gap rather than an attack, because
    /// monotonic time can stall while the machine is asleep. Either way the
    /// interval must not be credited, since the user was not present.
    Forward { by: Duration },
}

/// Detects wall-clock manipulation and sleep gaps by cross-checking wall time
/// against monotonic time.
pub struct ClockGuard {
    last_wall: DateTime<Utc>,
    last_mono: StdDuration,
    tolerance: Duration,
}

impl ClockGuard {
    /// `tolerance` absorbs NTP slew and scheduler jitter. A few seconds is
    /// right; too small and every NTP correction looks like an attack.
    pub fn new(clock: &dyn Clock, tolerance: Duration) -> Self {
        Self {
            last_wall: clock.now_utc(),
            last_mono: clock.monotonic(),
            tolerance,
        }
    }

    pub fn check(&mut self, clock: &dyn Clock) -> ClockVerdict {
        let wall = clock.now_utc();
        let mono = clock.monotonic();

        // Monotonic time cannot go backwards, so saturating_sub only guards
        // against a badly behaved Clock implementation.
        let mono_delta = mono.saturating_sub(self.last_mono);
        let wall_delta = wall - self.last_wall;

        let mono_delta_chrono =
            Duration::try_seconds(mono_delta.as_secs() as i64).unwrap_or_else(Duration::zero);
        let drift = wall_delta - mono_delta_chrono;

        self.last_wall = wall;
        self.last_mono = mono;

        if drift < -self.tolerance {
            ClockVerdict::Backward { by: -drift }
        } else if drift > self.tolerance {
            ClockVerdict::Forward { by: drift }
        } else {
            ClockVerdict::Normal {
                elapsed: mono_delta,
            }
        }
    }
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
    fn normal_passage_of_time_is_credited() {
        let clock = TestClock::new(at("2026-08-20T10:00:00Z"), 0);
        let mut guard = ClockGuard::new(&clock, Duration::seconds(2));

        clock.advance(StdDuration::from_secs(60));

        assert_eq!(
            guard.check(&clock),
            ClockVerdict::Normal {
                elapsed: StdDuration::from_secs(60)
            }
        );
    }

    #[test]
    fn winding_the_clock_back_is_detected() {
        let clock = TestClock::new(at("2026-08-20T10:00:00Z"), 0);
        let mut guard = ClockGuard::new(&clock, Duration::seconds(2));

        // User sets the clock back an hour hoping to reset their daily budget.
        clock.advance(StdDuration::from_secs(10));
        clock.skew_wall(Duration::hours(-1));

        match guard.check(&clock) {
            ClockVerdict::Backward { by } => {
                assert!(by >= Duration::minutes(59), "unexpected delta {by}");
            }
            other => panic!("expected Backward, got {other:?}"),
        }
    }

    #[test]
    fn sleep_gap_is_not_credited() {
        let clock = TestClock::new(at("2026-08-20T10:00:00Z"), 0);
        let mut guard = ClockGuard::new(&clock, Duration::seconds(2));

        // Machine suspends: wall time marches on, monotonic time stalls.
        clock.skew_wall(Duration::hours(8));

        match guard.check(&clock) {
            ClockVerdict::Forward { by } => {
                assert!(by >= Duration::hours(7), "unexpected delta {by}");
            }
            other => panic!("expected Forward, got {other:?}"),
        }
    }

    #[test]
    fn small_ntp_correction_is_tolerated() {
        let clock = TestClock::new(at("2026-08-20T10:00:00Z"), 0);
        let mut guard = ClockGuard::new(&clock, Duration::seconds(5));

        clock.advance(StdDuration::from_secs(30));
        clock.skew_wall(Duration::seconds(-1));

        assert!(matches!(guard.check(&clock), ClockVerdict::Normal { .. }));
    }
}
