//! Reconnect backoff for the persistent agent connection.
//!
//! Pure arithmetic, deliberately free of I/O: the pipe shell in `main` asks
//! "how long should I wait before the next attempt?" and applies the answer.
//! Keeping this out of the connect path means the retry policy is unit-testable
//! and cannot silently drift between "grow" and "reset" behaviour.

use std::time::Duration;

/// Capped exponential backoff: each failure doubles the wait until `cap`,
/// and one healthy exchange resets it to `base`.
///
/// Why capped: a session helper that retries every second against an agent
/// which is down for hours must not spin a core; why reset on success: a
/// single transient blip must not leave the helper sleeping seconds between
/// otherwise-healthy cycles once the agent is back.
#[derive(Debug, Clone)]
pub struct Backoff {
    base: Duration,
    cap: Duration,
    next: Duration,
}

impl Backoff {
    /// `base` is the first (and post-reset) delay; `cap` bounds every delay.
    pub fn new(base: Duration, cap: Duration) -> Self {
        assert!(base <= cap, "backoff base must not exceed its cap");
        Self {
            base,
            cap,
            next: base,
        }
    }

    /// Records a failed attempt and returns the delay to wait before the next.
    pub fn on_failure(&mut self) -> Duration {
        let waited = self.next;
        self.next = (self.next * 2).min(self.cap);
        waited
    }

    /// Records a successful exchange; the next failure starts from `base`.
    pub fn on_success(&mut self) {
        self.next = self.base;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_double_up_to_the_cap() {
        let mut b = Backoff::new(Duration::from_millis(250), Duration::from_secs(2));
        assert_eq!(b.on_failure(), Duration::from_millis(250));
        assert_eq!(b.on_failure(), Duration::from_millis(500));
        assert_eq!(b.on_failure(), Duration::from_secs(1));
        assert_eq!(b.on_failure(), Duration::from_secs(2));
        // Saturates instead of growing forever.
        assert_eq!(b.on_failure(), Duration::from_secs(2));
    }

    #[test]
    fn a_success_resets_backoff_to_base() {
        let mut b = Backoff::new(Duration::from_millis(250), Duration::from_secs(8));
        b.on_failure();
        b.on_failure();
        b.on_success();
        // Observable through the next failure's delay, which is what the loop
        // actually consumes.
        assert_eq!(b.on_failure(), Duration::from_millis(250));
    }
}
