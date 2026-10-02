//! Keeps the per-user session helper running.
//!
//! The helper is the agent's only source of focus reports, so while it is
//! gone nothing is enforced. It runs unprivileged in the user's session and
//! can be ended from Task Manager; the HKCU Run entry only starts it at the
//! next sign-in. The service therefore relaunches it when reports stop.
//!
//! This module is only the decision: *when* to try. Launching is the
//! caller's job (`st_win32::session_launch` in the service). The helper is
//! single-instance per user, so a launch that races a helper which is still
//! starting up costs one short-lived process and nothing else.

use chrono::{DateTime, Duration, Utc};

/// No launch attempts this soon after the agent starts: at boot the helper
/// is started by the user's own sign-in, and it needs a moment to connect.
pub const STARTUP_GRACE_SECS: i64 = 60;

/// The helper reports every second (empty batches included), so this much
/// silence means it is not running.
pub const SILENCE_SECS: i64 = 15;

/// First retry delay after an attempt; doubles per attempt up to the cap.
const FIRST_BACKOFF_SECS: i64 = 10;
const MAX_BACKOFF_SECS: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Wait,
    Launch,
}

#[derive(Debug)]
pub struct HelperSupervisor {
    next_attempt: DateTime<Utc>,
    backoff: Duration,
}

impl HelperSupervisor {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            next_attempt: now + Duration::seconds(STARTUP_GRACE_SECS),
            backoff: Duration::seconds(FIRST_BACKOFF_SECS),
        }
    }

    /// One tick. `helper_alive` is "a report arrived within
    /// [`SILENCE_SECS`]". Returns `Launch` at most once per backoff step.
    pub fn tick(&mut self, now: DateTime<Utc>, helper_alive: bool) -> Decision {
        if helper_alive {
            // Healthy again: the next outage starts from a short delay, but
            // still not instantly (the helper may be mid-reconnect).
            self.backoff = Duration::seconds(FIRST_BACKOFF_SECS);
            self.next_attempt = now + Duration::seconds(SILENCE_SECS);
            return Decision::Wait;
        }
        if now < self.next_attempt {
            return Decision::Wait;
        }
        self.next_attempt = now + self.backoff;
        self.backoff = (self.backoff * 2).min(Duration::seconds(MAX_BACKOFF_SECS));
        Decision::Launch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000 + secs, 0).unwrap()
    }

    #[test]
    fn waits_out_the_startup_grace_period() {
        let mut s = HelperSupervisor::new(t(0));
        for sec in 0..STARTUP_GRACE_SECS {
            assert_eq!(s.tick(t(sec), false), Decision::Wait, "at {sec}s");
        }
        assert_eq!(s.tick(t(STARTUP_GRACE_SECS), false), Decision::Launch);
    }

    #[test]
    fn retries_back_off_exponentially_up_to_the_cap() {
        let mut s = HelperSupervisor::new(t(0));
        let mut launches = Vec::new();
        for sec in STARTUP_GRACE_SECS..STARTUP_GRACE_SECS + 1200 {
            if s.tick(t(sec), false) == Decision::Launch {
                launches.push(sec - STARTUP_GRACE_SECS);
            }
        }
        // 0, +10, +20, +40, +80, +160, then every 300.
        assert_eq!(&launches[..7], &[0, 10, 30, 70, 150, 310, 610]);
        assert_eq!(launches[7] - launches[6], 300);
    }

    #[test]
    fn a_live_helper_is_never_relaunched_and_resets_the_backoff() {
        let mut s = HelperSupervisor::new(t(0));
        for sec in 0..600 {
            assert_eq!(s.tick(t(sec), true), Decision::Wait);
        }
        // Outage after a long healthy stretch: first retry comes quickly.
        let mut first = None;
        for sec in 600..700 {
            if s.tick(t(sec), false) == Decision::Launch {
                first = Some(sec);
                break;
            }
        }
        assert_eq!(first, Some(600 - 1 + SILENCE_SECS));
    }

    #[test]
    fn recovery_mid_backoff_starts_the_next_outage_fresh() {
        let mut s = HelperSupervisor::new(t(0));
        let mut sec = STARTUP_GRACE_SECS;
        let mut n = 0;
        while n < 4 {
            if s.tick(t(sec), false) == Decision::Launch {
                n += 1;
            }
            sec += 1;
        }
        // Helper comes back for a while, then dies again.
        for _ in 0..30 {
            s.tick(t(sec), true);
            sec += 1;
        }
        let died = sec;
        let mut launches = Vec::new();
        while launches.len() < 2 {
            if s.tick(t(sec), false) == Decision::Launch {
                launches.push(sec - died);
            }
            sec += 1;
        }
        assert_eq!(launches, vec![SILENCE_SECS - 1, SILENCE_SECS - 1 + 10]);
    }
}
