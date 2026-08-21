//! PIN hashing and verification, Argon2id.
//!
//! The PIN is the gate on anything that loosens enforcement: editing a limit,
//! granting an override, or changing the PIN itself. The plaintext PIN never
//! leaves the UI and never touches the database; only the PHC-formatted hash
//! is stored (in `settings`, via `st-storage`).

use argon2::password_hash::{PasswordHash, PasswordHasher, SaltString};
use argon2::{Argon2, PasswordVerifier};
use rand_core::OsRng;

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
}
