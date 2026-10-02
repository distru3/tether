//! The PIN vault: set, recover, remove, rotate.

use super::*;

pub(super) fn set_pin(ctx: &Ctx, new_pin: &str, current_pin: Option<&str>) -> Response {
    if new_pin.is_empty() {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "PIN cannot be empty".into(),
        };
    }
    // Changing an existing vault accepts either the current PIN or the
    // standing recovery code — same ownership proof either way. With no vault
    // yet, this passes and the first PIN is set.
    if let Err(denied) =
        auth::verify_credential(ctx, current_pin.unwrap_or(""), auth::Accept::PinOrRecovery)
    {
        return denied.into();
    }
    rotate_vault(ctx, new_pin)
}

/// Replace a forgotten PIN via its recovery code. Only the code unlocks this
/// path — by definition the user does not have the PIN — and the vault is
/// rotated so the used code stops working immediately.
pub(super) fn recover_pin(ctx: &Ctx, recovery_code: &str, new_pin: &str) -> Response {
    if new_pin.is_empty() {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "PIN cannot be empty".into(),
        };
    }
    if let Err(denied) = auth::verify_credential(ctx, recovery_code, auth::Accept::Recovery) {
        return denied.into();
    }
    rotate_vault(ctx, new_pin)
}

/// Dismantle the vault. Requires the same ownership proof as changing it
/// (PIN or recovery code); idempotent when no vault exists.
pub(super) fn remove_pin(ctx: &Ctx, credential: &str) -> Response {
    if let Err(denied) = auth::verify_credential(ctx, credential, auth::Accept::PinOrRecovery) {
        return denied.into();
    }
    let db = lock_db(&ctx.db);
    match db
        .clear_pin_vault()
        .and_then(|()| db.audit(ctx.clock.now_utc(), "pin_removed", None))
    {
        Ok(()) => accepted(ctx.clock.now_utc()),
        Err(e) => error_internal(format!("clear vault: {e}")),
    }
}

/// Store a new PIN together with a freshly minted recovery code.
///
/// The plaintext code exists exactly once — inside the [`Response::PinVault`]
/// returned here — because only its hash is persisted. Every rotation retires
/// the previous code, so stale codes from earlier eras are inert. Both Argon2
/// hashes are computed before the database lock is taken.
pub(super) fn rotate_vault(ctx: &Ctx, new_pin: &str) -> Response {
    let pin_hash = match hash_pin(new_pin) {
        Ok(h) => h,
        Err(e) => return error_internal(format!("hash pin: {e}")),
    };
    let code = generate_recovery_code();
    let canonical = normalize_recovery_code(&code);
    let recovery_hash = match hash_pin(&canonical) {
        Ok(h) => h,
        Err(e) => return error_internal(format!("hash recovery: {e}")),
    };
    let db = lock_db(&ctx.db);
    match db
        .set_pin_hash(&pin_hash)
        .and_then(|()| db.set_recovery_hash(&recovery_hash))
        .and_then(|()| db.audit(ctx.clock.now_utc(), "pin_set", None))
    {
        Ok(()) => Response::PinVault {
            recovery_code: code,
        },
        Err(e) => error_internal(format!("store vault: {e}")),
    }
}
