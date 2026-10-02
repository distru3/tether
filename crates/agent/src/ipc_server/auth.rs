//! Central authorization for IPC requests.
//!
//! # Why one gate
//!
//! The pipe ACL admits every authenticated local user — the UI and the session
//! helper run as the (possibly supervised) user — so the pipe itself proves
//! nothing about who is asking. Every request that loosens enforcement must
//! therefore carry the PIN, and the check used to be repeated (or forgotten)
//! inside each handler: `SetSetting`, `Categorize`, schedule edits and the
//! allowlist had no check at all. [`authorize`] runs before dispatch and is
//! the single place that decides which requests need a credential.
//!
//! # Rules
//!
//! * Requests that always need the PIN (once one is configured): quitting a
//!   blocked app, limit edits, overrides, removing a web block, recategorising
//!   an app, editing or deleting a schedule.
//! * Direction-dependent: disabling a schedule, allowlisting a subject, and any
//!   setting change [`SettingKey::is_loosening`] flags. The opposite direction
//!   (enabling, un-allowlisting, switching protection on) is free.
//! * Overrides are refused outright in strict mode, before any PIN is spent.
//! * Quitting a *blocked* app is free; quitting any other app needs the PIN.
//! * `ReportUsage` / `RegisterDiscoveredApps` are refused from any program
//!   other than the session helper ([`Peer`]).
//! * The vault requests (`SetPin`, `RecoverPin`, `RemovePin`) carry their own
//!   credential semantics and call [`verify_credential`] from their handlers.
//!
//! No PIN configured means every check passes: the product lets a household
//! use the app before choosing a PIN.
//!
//! # Brute force
//!
//! Every non-empty wrong credential counts against a [`PinThrottle`]; once it
//! trips, checks are refused with [`ErrorCode::RateLimited`] until the lockout
//! ends. An *empty* credential means "not supplied" (the UI probes without a
//! PIN and prompts only when told one is needed), so it never counts.
//!
//! Argon2 verification is deliberately done *without* the database lock held:
//! each check costs tens of milliseconds, and holding the lock would stall the
//! 1 Hz enforcement loop and every other client for the duration.

use chrono::{DateTime, Utc};
use st_core::pin::{normalize_recovery_code, verify_pin};
use st_core::settings::SettingKey;
use st_ipc::{ErrorCode, Request, Response};

use super::Ctx;
use crate::locks::{lock_db, lock_recover, read_recover};

/// Why a request was refused. Deliberately small (clippy's
/// `result_large_err`): it converts into the wire [`Response::Error`] only at
/// the dispatch boundary.
#[derive(Debug)]
pub(super) struct Denied {
    code: ErrorCode,
    message: String,
}

impl Denied {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn internal(message: String) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}

impl From<Denied> for Response {
    fn from(denied: Denied) -> Self {
        Response::Error {
            code: denied.code,
            message: denied.message,
        }
    }
}

/// Who is on the other end of the pipe, resolved once per connection from the
/// peer process's executable (see `st_win32::peer_is_trusted`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Peer {
    /// Tether's own session helper.
    SessionHelper,
    /// Some other program: the UI, or anything else a local user runs.
    Other,
    /// The peer could not be inspected (it may have exited already).
    Unknown,
}

impl Peer {
    #[cfg(windows)]
    pub(crate) fn of(stream: &st_ipc::transport::PipeStream) -> Self {
        match st_win32::peer_is_trusted(stream.peer_process_id(), &["screentime-session.exe"]) {
            Some(true) => Peer::SessionHelper,
            Some(false) => Peer::Other,
            None => Peer::Unknown,
        }
    }
}

/// Which credentials a check accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Accept {
    Pin,
    Recovery,
    PinOrRecovery,
}

/// Decide whether `request` may run. `Err` carries the response to send back.
pub(super) fn authorize(ctx: &Ctx, peer: Peer, request: &Request) -> Result<(), Denied> {
    match request {
        // Only the session helper sees the focused window. Usage or focus
        // reported by anything else would let a local process credit the
        // wrong app, or point the enforcer away from a blocked one.
        // An uninspectable peer is allowed (logged at the connection) so a
        // lookup hiccup can never silently stop usage tracking.
        Request::ReportUsage { .. } | Request::RegisterDiscoveredApps { .. }
            if peer == Peer::Other =>
        {
            Err(Denied::new(
                ErrorCode::BadRequest,
                "usage reports are accepted only from the session helper",
            ))
        }

        // Quitting an app that is currently blocked only tightens things, so
        // the block screen's Quit button works without a PIN. Closing any
        // other app is still PIN-gated: CloseApps can terminate arbitrary
        // processes by app id.
        Request::CloseApps { app_id, pin } => {
            let blocked = lock_db(&ctx.db)
                .is_blocked(st_core::model::SubjectRef::App(*app_id))
                .map_err(|e| Denied::internal(format!("read block state: {e}")))?;
            if blocked {
                Ok(())
            } else {
                require_pin(ctx, pin)
            }
        }

        Request::SetLimit { pin, .. }
        | Request::DeleteLimit { pin, .. }
        | Request::CancelPendingLimit { pin, .. }
        | Request::RemoveManualBlock { pin, .. }
        | Request::VerifyPin { pin } => require_pin(ctx, pin),

        Request::GrantOverride { pin, .. } => {
            if read_recover(&ctx.policy, "policy").strict_mode {
                return Err(Denied::new(
                    ErrorCode::StrictMode,
                    "overrides are disabled in strict mode",
                ));
            }
            require_pin(ctx, pin)
        }

        Request::Categorize { pin, .. }
        | Request::UpdateSchedule { pin, .. }
        | Request::DeleteSchedule { pin, .. } => require_pin(ctx, supplied(pin)),

        Request::SetScheduleEnabled {
            enabled: false,
            pin,
            ..
        }
        | Request::SetAllowlist {
            allowed: true, pin, ..
        } => require_pin(ctx, supplied(pin)),

        Request::SetSetting { key, value, pin } => {
            let (key, value) = parse_setting(key, value)?;
            let current = lock_db(&ctx.db)
                .setting(key.storage_key())
                .map_err(|e| Denied::internal(format!("read setting: {e}")))?;
            if key.is_loosening(current.as_deref(), &value) {
                require_pin(ctx, supplied(pin))
            } else {
                Ok(())
            }
        }

        _ => Ok(()),
    }
}

/// Parse and validate a `SetSetting` pair. Shared with the handler so both see
/// exactly the same canonical value.
pub(super) fn parse_setting(key: &str, value: &str) -> Result<(SettingKey, String), Denied> {
    let Some(parsed) = SettingKey::parse(key) else {
        return Err(Denied::new(
            ErrorCode::BadRequest,
            format!("unknown setting: {key}"),
        ));
    };
    let value = parsed
        .normalize(value)
        .map_err(|reason| Denied::new(ErrorCode::BadRequest, reason))?;
    Ok((parsed, value))
}

fn supplied(pin: &Option<String>) -> &str {
    pin.as_deref().unwrap_or("")
}

fn require_pin(ctx: &Ctx, pin: &str) -> Result<(), Denied> {
    verify_credential(ctx, pin, Accept::Pin)
}

/// Check `supplied` against the stored vault.
///
/// * No vault configured: `Ok` for [`Accept::Pin`] and
///   [`Accept::PinOrRecovery`]; `NotFound` for [`Accept::Recovery`] (there is
///   nothing to recover).
/// * Empty `supplied` with a vault: `BadPin`, without counting as a failure.
/// * Otherwise verified outside the database lock and recorded in the
///   throttle; a wrong credential is also written to the audit log.
pub(super) fn verify_credential(ctx: &Ctx, supplied: &str, accept: Accept) -> Result<(), Denied> {
    let now = ctx.clock.now_utc();
    if let Some(until) = lock_recover(&ctx.live.pin_throttle, "pin throttle").locked_until(now) {
        return Err(rate_limited(until, now));
    }

    let (pin_hash, recovery_hash) = {
        let db = lock_db(&ctx.db);
        let pin = db
            .pin_hash()
            .map_err(|e| Denied::internal(format!("read pin: {e}")))?;
        let recovery = db
            .recovery_hash()
            .map_err(|e| Denied::internal(format!("read recovery: {e}")))?;
        (pin, recovery)
    };

    match (accept, &pin_hash, &recovery_hash) {
        (Accept::Recovery, _, None) => {
            return Err(Denied::new(
                ErrorCode::NotFound,
                "no PIN vault is configured",
            ))
        }
        (Accept::Pin | Accept::PinOrRecovery, None, _) => return Ok(()),
        _ => {}
    }

    if supplied.is_empty() {
        return Err(bad_pin(accept));
    }

    let pin_matches = || pin_hash.as_deref().is_some_and(|h| verify_pin(supplied, h));
    let recovery_matches = || {
        recovery_hash
            .as_deref()
            .is_some_and(|h| verify_pin(&normalize_recovery_code(supplied), h))
    };
    let ok = match accept {
        Accept::Pin => pin_matches(),
        Accept::Recovery => recovery_matches(),
        Accept::PinOrRecovery => pin_matches() || recovery_matches(),
    };

    let mut throttle = lock_recover(&ctx.live.pin_throttle, "pin throttle");
    if ok {
        throttle.record_success();
        return Ok(());
    }
    throttle.record_failure(now);
    let locked = throttle.locked_until(now);
    drop(throttle);

    let detail = match locked {
        Some(until) => format!("{accept:?}; locked until {}", until.to_rfc3339()),
        None => format!("{accept:?}"),
    };
    if let Err(e) = lock_db(&ctx.db).audit(now, "credential_rejected", Some(&detail)) {
        tracing::warn!(error = %e, "failed to audit rejected credential");
    }
    tracing::warn!(?accept, locked = locked.is_some(), "rejected credential");
    Err(bad_pin(accept))
}

fn bad_pin(accept: Accept) -> Denied {
    let message = match accept {
        Accept::Pin => "PIN required",
        Accept::Recovery => "recovery code does not match",
        Accept::PinOrRecovery => "wrong PIN or recovery code",
    };
    Denied::new(ErrorCode::BadPin, message)
}

fn rate_limited(until: DateTime<Utc>, now: DateTime<Utc>) -> Denied {
    let secs = (until - now).num_seconds().max(1);
    Denied::new(
        ErrorCode::RateLimited,
        format!("too many wrong attempts; try again in {secs} seconds"),
    )
}
