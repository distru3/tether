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
/// Groups of 4 characters, 4 groups — 64^16 of entropy, writable on paper.
const RECOVERY_GROUPS: usize = 4;
const RECOVERY_GROUP_LEN: usize = 4;

/// Generate a fresh recovery code, e.g. `K7M2-QP9X-4RTA-B38D`.
///
/// Drawn from the OS CSPRNG. The caller shows it once and stores only
/// [`hash_pin`] of it; this function deliberately never persists anything.
pub fn generate_recovery_code() -> String {
    let mut raw = [0u8; RECOVERY_GROUPS * RECOVERY_GROUP_LEN];
    OsRng.fill_bytes(&mut raw);
    let chars: String = raw
        .iter()
        .map(|&b| RECOVERY_ALPHABET[(b as usize) % RECOVERY_ALPHABET.len()] as char)
        .collect();
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
}
