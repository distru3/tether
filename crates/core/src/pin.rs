//! PIN hashing and verification, Argon2id.
//!
//! The PIN is the gate on anything that loosens enforcement: editing a limit,
//! granting an override, or changing the PIN itself. The plaintext PIN never
//! leaves the UI and never touches the database; only the PHC-formatted hash
//! is stored (in `settings`, via `st-storage`).
//!
//! Recovery: every time a PIN is set or changed, a one-time **recovery code**
//! is generated and shown to the user exactly once; only its Argon2id hash is
//! stored. The code can stand in for the PIN when setting a new one after a
//! forget, or when removing the vault entirely. It rotates on every vault
//! change, so a code from an earlier era stops working the moment the vault
//! changes.

use argon2::password_hash::{PasswordHash, PasswordHasher, SaltString};
use argon2::{Argon2, PasswordVerifier};
use chrono::{DateTime, Duration, Utc};
use rand_core::{OsRng, RngCore};

/// Hash a PIN into a PHC string (e.g. `$argon2id$v=19$m=19456,t=2,p=1$...`).
///
/// A fresh random salt is generated per call, so hashing the same PIN twice
/// yields different hashes. Always succeeds unless the argon2 params are
/// somehow invalid, which is a programmer error.
pub fn hash_pin(pin: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default().hash_password(pin.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Verify a PIN against a stored hash. Returns `false` on any mismatch or on a
/// malformed stored hash — a corrupt vault is treated as "wrong PIN", never an
/// error, so the agent cannot be made to fail open.
pub fn verify_pin(pin: &str, stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return false;
    };
    Argon2::default()
        .verify_password(pin.as_bytes(), &parsed)
        .is_ok()
}

/// Unambiguous alphabet for human transcription: no 0/O or 1/I/L pairs.
const RECOVERY_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
/// Groups of 4 characters, 4 groups — 31^16 (~79 bits) of entropy, writable on
/// paper.
const RECOVERY_GROUPS: usize = 4;
const RECOVERY_GROUP_LEN: usize = 4;

/// Generate a fresh recovery code, e.g. `K7M2-QP9X-4RTA-B38D`.
///
/// Drawn from the OS CSPRNG. The caller shows it once and stores only
/// [`hash_pin`] of it; this function deliberately never persists anything.
pub fn generate_recovery_code() -> String {
    // Rejection sampling: 256 is not a multiple of the alphabet size, so a
    // plain `byte % len` would favour the first few characters. Bytes at or
    // above the largest multiple of `len` are redrawn instead.
    let len = RECOVERY_ALPHABET.len();
    let limit = 256 - (256 % len);
    let mut chars = String::with_capacity(RECOVERY_GROUPS * RECOVERY_GROUP_LEN);
    let mut buf = [0u8; 32];
    while chars.len() < RECOVERY_GROUPS * RECOVERY_GROUP_LEN {
        OsRng.fill_bytes(&mut buf);
        for &b in &buf {
            if (b as usize) < limit && chars.len() < RECOVERY_GROUPS * RECOVERY_GROUP_LEN {
                chars.push(RECOVERY_ALPHABET[(b as usize) % len] as char);
            }
        }
    }
    (0..RECOVERY_GROUPS)
        .map(|g| &chars[g * RECOVERY_GROUP_LEN..(g + 1) * RECOVERY_GROUP_LEN])
        .collect::<Vec<_>>()
        .join("-")
}

/// Normalise user-typed input into canonical code form: uppercase, whitespace
/// stripped, dashes optional — so `k7m2 qp9x 4rta b38d` matches its paper form.
pub fn normalize_recovery_code(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect::<Vec<_>>()
        .chunks(RECOVERY_GROUP_LEN)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

/// Consecutive wrong credentials tolerated before lockouts begin.
pub const THROTTLE_FREE_ATTEMPTS: u32 = 5;
/// Lockout after the first failure past the free attempts; doubles with each
/// further failure.
pub const THROTTLE_BASE_LOCKOUT_SECS: i64 = 30;
/// Ceiling for a single lockout.
pub const THROTTLE_MAX_LOCKOUT_SECS: i64 = 15 * 60;

/// Brute-force throttle for PIN and recovery-code checks.
///
/// A 4-6 digit PIN has at most a million candidates, and the pipe accepts up
/// to 32 concurrent clients, so without a throttle a local script could walk
/// the PIN space in hours. After [`THROTTLE_FREE_ATTEMPTS`] consecutive
/// failures every further failure locks credential checks for
/// [`THROTTLE_BASE_LOCKOUT_SECS`] doubling up to [`THROTTLE_MAX_LOCKOUT_SECS`];
/// a success resets the count. Time is injected, so the policy is testable and
/// survives clock rules the agent already enforces elsewhere.
///
/// State is in memory: an agent restart forgives past failures. Restarting the
/// agent needs administrator rights, which is already game over for the
/// enforcement model, so persisting the counter would add nothing.
#[derive(Debug, Clone, Default)]
pub struct PinThrottle {
    consecutive_failures: u32,
    locked_until: Option<DateTime<Utc>>,
}

impl PinThrottle {
    /// `Some(deadline)` while credential checks are refused.
    pub fn locked_until(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.locked_until.filter(|until| *until > now)
    }

    pub fn record_failure(&mut self, now: DateTime<Utc>) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.consecutive_failures > THROTTLE_FREE_ATTEMPTS {
            let doublings = (self.consecutive_failures - THROTTLE_FREE_ATTEMPTS - 1).min(16);
            let secs = (THROTTLE_BASE_LOCKOUT_SECS << doublings).min(THROTTLE_MAX_LOCKOUT_SECS);
            self.locked_until = Some(now + Duration::seconds(secs));
        }
    }

    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.locked_until = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_round_trip() {
        let stored = hash_pin("1234").expect("hash");
        assert!(verify_pin("1234", &stored));
        assert!(!verify_pin("0000", &stored));
    }

    #[test]
    fn hashes_are_salted_and_do_not_repeat() {
        let a = hash_pin("1234").expect("a");
        let b = hash_pin("1234").expect("b");
        assert_ne!(a, b, "each hash must use a fresh salt");
    }

    #[test]
    fn a_malformed_stored_hash_verifies_as_false() {
        assert!(!verify_pin("1234", "not-a-real-hash"));
        assert!(!verify_pin("1234", ""));
    }

    #[test]
    fn recovery_codes_match_their_documented_shape() {
        let code = generate_recovery_code();
        let groups: Vec<&str> = code.split('-').collect();
        assert_eq!(groups.len(), RECOVERY_GROUPS);
        for group in groups {
            assert_eq!(group.len(), RECOVERY_GROUP_LEN);
            assert!(
                group
                    .chars()
                    .all(|c| RECOVERY_ALPHABET.contains(&(c as u8))),
                "group {group} outside the unambiguous alphabet"
            );
        }
    }

    #[test]
    fn recovery_codes_are_random() {
        let a = generate_recovery_code();
        let b = generate_recovery_code();
        assert_ne!(a, b, "two CSPRNG draws colliding is effectively impossible");
    }

    #[test]
    fn normalization_tolerates_human_input() {
        let code = generate_recovery_code();
        let sloppy = code
            .to_lowercase()
            .chars()
            .flat_map(|c| [c, ' '])
            .collect::<String>();
        assert_eq!(normalize_recovery_code(&sloppy), code);
        assert_eq!(normalize_recovery_code(&code.replace('-', "")), code);
    }

    fn t(secs: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_800_000_000 + secs, 0).expect("valid")
    }

    #[test]
    fn the_first_failures_are_free() {
        let mut throttle = PinThrottle::default();
        for _ in 0..THROTTLE_FREE_ATTEMPTS {
            throttle.record_failure(t(0));
        }
        assert_eq!(throttle.locked_until(t(0)), None);
    }

    #[test]
    fn lockouts_double_and_cap() {
        let mut throttle = PinThrottle::default();
        for _ in 0..THROTTLE_FREE_ATTEMPTS {
            throttle.record_failure(t(0));
        }
        throttle.record_failure(t(0));
        assert_eq!(throttle.locked_until(t(0)), Some(t(30)));
        throttle.record_failure(t(100));
        assert_eq!(throttle.locked_until(t(100)), Some(t(160)));
        for _ in 0..20 {
            throttle.record_failure(t(1000));
        }
        assert_eq!(
            throttle.locked_until(t(1000)),
            Some(t(1000 + THROTTLE_MAX_LOCKOUT_SECS))
        );
    }

    #[test]
    fn a_lockout_expires_and_success_resets_the_count() {
        let mut throttle = PinThrottle::default();
        for _ in 0..=THROTTLE_FREE_ATTEMPTS {
            throttle.record_failure(t(0));
        }
        assert!(throttle.locked_until(t(29)).is_some());
        assert!(throttle.locked_until(t(30)).is_none(), "expired");
        throttle.record_success();
        throttle.record_failure(t(40));
        assert!(throttle.locked_until(t(40)).is_none(), "count was reset");
    }

    #[test]
    fn recovery_codes_use_only_the_alphabet() {
        for _ in 0..50 {
            let code = generate_recovery_code();
            assert!(code
                .chars()
                .filter(|c| *c != '-')
                .all(|c| RECOVERY_ALPHABET.contains(&(c as u8))));
        }
    }
}
