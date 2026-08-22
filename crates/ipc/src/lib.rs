//! Wire protocol between the UI, the session helper and the privileged agent.
//!
//! Deliberately transport-agnostic: framing works over anything implementing
//! `Read`/`Write`, so the same code serves a Windows named pipe and a Unix
//! domain socket, and tests can use an in-memory buffer.
//!
//! # Security posture
//!
//! The agent is the only component that may touch the database, the hosts file
//! or another process. Everything else asks. That is what makes "just edit the
//! database" not the easiest bypass, so this boundary must stay narrow:
//!
//! * The transport is local only. Never bind a TCP socket.
//! * The agent authenticates the peer at accept time (pipe ACL on Windows,
//!   `SO_PEERCRED` on Linux) and again per privileged request.
//! * Anything that loosens enforcement ([`Request::SetLimit`],
//!   [`Request::GrantOverride`]) carries a PIN and is written to the audit log
//!   whether it succeeds or fails.

use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use st_core::daykey::DayKey;
use thiserror::Error;

pub mod transport;

/// Rejects oversized frames before allocating, so a malformed or hostile length
/// prefix cannot exhaust memory.
pub const MAX_FRAME_BYTES: u32 = 8 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed message: {0}")]
    Codec(#[from] serde_json::Error),
    #[error("frame of {size} bytes exceeds the {MAX_FRAME_BYTES} byte limit")]
    FrameTooLarge { size: u32 },
    #[error("peer closed the connection")]
    Closed,
}

pub type Result<T> = std::result::Result<T, IpcError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Liveness probe. Also how the UI detects that the agent has been stopped.
    Ping,
    /// Dashboard payload for one day.
    DaySummary { day: DayKey },
    /// Enforcement state and which capabilities are actually available, so the
    /// UI can be honest about degraded modes.
    Status,
    /// Reclassify an app. `primary` drives reporting; `tags` only affect limits.
    Categorize {
        app_id: i64,
        primary: i64,
        tags: Vec<i64>,
    },
    /// Everything the limit editor needs in one round trip: apps, categories
    /// and current limits.
    Catalog,
    /// Which apps are currently blocked, for the overlay owner (the session
    /// helper). Polled, so the session helper knows when to show/hide its
    /// overlay.
    BlockedApps,
    /// Deliberate user action from the block overlay's "Quit" button:
    /// terminate the app's process tree. Distinct from the removed auto-freeze;
    /// this only happens when the user chooses to quit.
    CloseApps { app_id: i64, pin: String },
    /// Create or update a limit. Tightening applies at once; loosening is
    /// subject to the cooldown, which is why the response carries an effective
    /// time rather than just success.
    SetLimit {
        target: LimitTargetDto,
        default_minutes: u32,
        weekday_minutes: [Option<u32>; 7],
        enabled: bool,
        pin: String,
    },
    /// Remove a limit. Loosening, so always subject to the cooldown.
    DeleteLimit { target: LimitTargetDto, pin: String },
    /// "+15 minutes", PIN gated, refused outright in strict mode.
    GrantOverride {
        target: LimitTargetDto,
        seconds: i64,
        pin: String,
    },
    /// Set or change the PIN. `current_pin` must match the existing PIN once
    /// one is configured; the first set (no PIN yet) can pass `None`.
    SetPin {
        new_pin: String,
        current_pin: Option<String>,
    },
    /// Usage reported by the session helper, which is the only component able to
    /// see the focused window.
    ReportUsage {
        app_key: String,
        display_name: String,
        seconds: i64,
        session_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Pong,
    DaySummary(DaySummaryDto),
    Status(StatusDto),
    Catalog(CatalogDto),
    BlockedApps(Vec<BlockedAppDto>),
    /// Accepted, with the instant the change actually takes effect. Equal to
    /// "now" for tightening, up to 24 hours out for loosening.
    Accepted {
        effective_utc: String,
    },
    Error {
        code: ErrorCode,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadPin,
    /// Rejected because the anti-impulse cooldown has not elapsed.
    CooldownActive,
    /// Refused because strict mode is on.
    StrictMode,
    /// The category is `never_block`, so it cannot carry a limit.
    NotLimitable,
    NotFound,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LimitTargetDto {
    App { id: i64 },
    Category { id: i64 },
    Total,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaySummaryDto {
    pub day: DayKey,
    pub total_seconds: i64,
    /// Per-app usage, already sorted descending by the agent so the UI does no
    /// work on the main thread.
    pub apps: Vec<UsageRowDto>,
    /// Per-primary-category usage. Sums to `total_seconds`; tags are excluded
    /// here precisely so the chart cannot exceed 100%.
    pub categories: Vec<UsageRowDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRowDto {
    pub id: i64,
    pub label: String,
    pub seconds: i64,
    pub color: Option<String>,
    /// Present when a limit applies, for the progress ring.
    pub limit_seconds: Option<i64>,
    pub blocked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusDto {
    pub agent_version: String,
    pub tracker_backend: String,
    pub enforcement_backend: String,
    pub filter_backend: String,
    /// False under Wayland, or wherever the tracker cannot see the focused
    /// window. The UI must show a banner rather than an empty dashboard.
    pub tracking_available: bool,
    pub blocks_encrypted_dns: bool,
    pub strict_mode: bool,
    pub pin_configured: bool,
}

/// Everything the limit editor needs, in one round trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogDto {
    pub apps: Vec<AppDto>,
    pub categories: Vec<CategoryDto>,
    pub limits: Vec<LimitDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppDto {
    pub id: i64,
    pub key: String,
    pub display_name: String,
    pub primary_category: i64,
    pub tags: Vec<i64>,
    pub user_classified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryDto {
    pub id: i64,
    pub slug: String,
    pub name: String,
    /// `limitable` | `block_only` | `never_block`.
    pub kind: String,
    pub color: String,
    pub builtin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitDto {
    pub id: i64,
    pub target: LimitTargetDto,
    pub default_minutes: u32,
    pub weekday_minutes: [Option<u32>; 7],
    pub enabled: bool,
}

/// A currently-blocked app, as reported to the overlay owner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedAppDto {
    pub app_id: i64,
    pub label: String,
    /// Canonical `kind:value` app key, so the session helper can match the
    /// focused window to the blocked app.
    pub app_key: String,
}

/// Write one length-prefixed JSON frame.
pub fn write_message<W: Write, T: Serialize>(writer: &mut W, message: &T) -> Result<()> {
    let payload = serde_json::to_vec(message)?;
    let size =
        u32::try_from(payload.len()).map_err(|_| IpcError::FrameTooLarge { size: u32::MAX })?;
    if size > MAX_FRAME_BYTES {
        return Err(IpcError::FrameTooLarge { size });
    }
    writer.write_all(&size.to_le_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

/// Read one length-prefixed JSON frame.
pub fn read_message<R: Read, T: for<'de> Deserialize<'de>>(reader: &mut R) -> Result<T> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Err(IpcError::Closed),
        Err(e) => return Err(IpcError::Io(e)),
    }

    let size = u32::from_le_bytes(len_buf);
    if size > MAX_FRAME_BYTES {
        return Err(IpcError::FrameTooLarge { size });
    }

    let mut payload = vec![0u8; size as usize];
    reader.read_exact(&mut payload)?;
    Ok(serde_json::from_slice(&payload)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn request_round_trips() {
        let mut buf = Vec::new();
        write_message(
            &mut buf,
            &Request::DaySummary {
                day: DayKey(20260820),
            },
        )
        .expect("write");

        let mut cursor = Cursor::new(buf);
        let decoded: Request = read_message(&mut cursor).expect("read");
        assert!(matches!(
            decoded,
            Request::DaySummary { day } if day == DayKey(20260820)
        ));
    }

    #[test]
    fn multiple_frames_stream_in_order() {
        let mut buf = Vec::new();
        write_message(&mut buf, &Request::Ping).expect("w1");
        write_message(&mut buf, &Request::Status).expect("w2");

        let mut cursor = Cursor::new(buf);
        assert!(matches!(
            read_message::<_, Request>(&mut cursor),
            Ok(Request::Ping)
        ));
        assert!(matches!(
            read_message::<_, Request>(&mut cursor),
            Ok(Request::Status)
        ));
    }

    #[test]
    fn a_clean_close_is_distinguishable_from_an_error() {
        let mut cursor = Cursor::new(Vec::new());
        assert!(matches!(
            read_message::<_, Request>(&mut cursor),
            Err(IpcError::Closed)
        ));
    }

    #[test]
    fn an_absurd_length_prefix_is_rejected_before_allocating() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&u32::MAX.to_le_bytes());
        let mut cursor = Cursor::new(buf);
        assert!(matches!(
            read_message::<_, Request>(&mut cursor),
            Err(IpcError::FrameTooLarge { .. })
        ));
    }

    #[test]
    fn truncated_payload_is_an_error_not_a_partial_parse() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&64u32.to_le_bytes());
        buf.extend_from_slice(b"{\"type\":\"ping\"");
        let mut cursor = Cursor::new(buf);
        assert!(read_message::<_, Request>(&mut cursor).is_err());
    }

    #[test]
    fn error_responses_serialise_with_stable_codes() {
        let json = serde_json::to_string(&Response::Error {
            code: ErrorCode::BadPin,
            message: "nope".into(),
        })
        .expect("serialise");
        assert!(json.contains("\"bad_pin\""), "unexpected payload: {json}");
    }

    #[test]
    fn blocked_apps_request_round_trips() {
        let mut buf = Vec::new();
        write_message(&mut buf, &Request::BlockedApps).expect("write");
        let mut cursor = Cursor::new(buf);
        assert!(matches!(
            read_message::<_, Request>(&mut cursor),
            Ok(Request::BlockedApps)
        ));
    }

    #[test]
    fn close_apps_request_round_trips_with_pin() {
        let mut buf = Vec::new();
        write_message(
            &mut buf,
            &Request::CloseApps {
                app_id: 6,
                pin: "1234".into(),
            },
        )
        .expect("write");
        let mut cursor = Cursor::new(buf);
        assert!(matches!(
            read_message::<_, Request>(&mut cursor),
            Ok(Request::CloseApps { app_id: 6, pin }) if pin == "1234"
        ));
    }

    #[test]
    fn blocked_app_dto_serialises_with_stable_fields() {
        let json = serde_json::to_string(&BlockedAppDto {
            app_id: 6,
            label: "Elden Ring".into(),
            app_key: "win-exe:c:\\games\\elden ring\\game\\eldenring.exe".into(),
        })
        .expect("serialise");
        assert!(json.contains("\"app_id\":6"));
        assert!(json.contains("Elden Ring"));
        assert!(json.contains("win-exe:"));
    }
}
