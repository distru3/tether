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
use st_core::model::AppKey;
use thiserror::Error;
use ts_rs::TS;

pub mod transport;

/// Rejects oversized frames before allocating, so a malformed or hostile length
/// prefix cannot exhaust memory.
pub const MAX_FRAME_BYTES: u32 = 8 * 1024 * 1024;

/// Canonical local endpoint name shared by the agent (server), the session
/// helper and the UI host (clients).
///
/// The transport functions take this *bare* name: on Windows,
/// [`transport::server_accept`] / [`transport::client_connect`] build the full
/// NT path `\\.\pipe\{PIPE_NAME}` from it. A future Unix port would place a
/// socket named after the same constant, so clients keep one source of truth.
/// The duplicates in `agent`/`session`/`src-tauri` are legacy and should be
/// replaced by this constant when those crates are rewired.
pub const PIPE_NAME: &str = "screentime";

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

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
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
        #[ts(as = "i32")]
        app_id: i64,
        #[ts(as = "i32")]
        primary: i64,
        #[ts(as = "Vec<i32>")]
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
    CloseApps {
        #[ts(as = "i32")]
        app_id: i64,
        pin: String,
    },
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
        #[ts(as = "i32")]
        seconds: i64,
        pin: String,
    },
    /// Set or change the PIN. `current_pin` must match the existing PIN once
    /// one is configured; the first set (no PIN yet) can pass `None`.
    SetPin {
        new_pin: String,
        current_pin: Option<String>,
    },
    /// Usage reported by the session helper, which is the only component able
    /// to see the focused window. See [`ReportUsageDto`] for the contract.
    ReportUsage { report: ReportUsageDto },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Pong,
    DaySummary(DaySummaryDto),
    Status(StatusDto),
    Catalog(CatalogDto),
    BlockedApps(BlockedAppsDto),
    /// Accepted, with the instant the change actually takes effect. Equal to
    /// "now" for tightening, up to 24 hours out for loosening.
    ///
    /// Also the acknowledgement for [`Request::ReportUsage`], where
    /// `effective_utc` carries the agent's *ingest* instant (RFC 3339) rather
    /// than an enforcement time: the session helper can compare it against its
    /// own send clock to detect and compensate for skew.
    Accepted {
        effective_utc: String,
    },
    Error {
        code: ErrorCode,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
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
    /// The request itself was invalid: a malformed payload, an out-of-range
    /// value or an unknown target id. Callers previously abused [`ErrorCode::
    /// BadPin`] for "unknown target"; use this instead so the UI can say
    /// something honest.
    BadRequest,
    Internal,
}

/// A batch of foreground-window observations from the session helper.
///
/// # Contract
///
/// * **Sampling.** The helper samples the focused window locally at ~1 Hz and
///   collapses consecutive identical samples (same app, same idle bucket) into
///   single observations carrying the accumulated seconds, so a two-hour
///   session is one observation rather than 7200. `window_title` is sampled at
///   most once per collapse window; it is advisory (shown in the UI), never
///   part of app identity.
/// * **Batching.** Observations are buffered client-side and flushed as one
///   `ReportUsage` request every few seconds of activity (and once before the
///   helper exits). Batches bound pipe traffic without delaying data much;
///   the agent must not rely on batch boundaries for anything.
/// * **Ordering.** Within a batch, observations are ordered ascending by
///   `observed_at_utc`. Across batches, ordering is only guaranteed per
///   connection; the agent tolerates interleaved helpers (fast user
///   switching) by attributing usage per `app_key` + day.
/// * **Idempotency.** The transport is one-request-per-connection, so a lost
///   response forces a resend that may double-deliver an entire batch. Ingestion
///   must therefore be idempotent: identical `(app_key, observed_at_utc)`
///   pairs collapse, and seconds accumulate additively otherwise.
/// * **Clocks.** `observed_at_utc` uses the helper's wall clock and is trusted
///   for day-bucketing only after the agent sanity-checks it against its own
///   receive time ([`Response::Accepted`]'s `effective_utc` lets the helper
///   measure skew). Large divergence is clamped, not trusted.
///
/// The agent replies with [`Response::Accepted`] on success; any
/// [`Response::Error`] means the whole batch was discarded and should be
/// retried after backoff (it was not partially applied).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReportUsageDto {
    pub observations: Vec<ObservationDto>,
}

/// One collapsed foreground-window sample taken by the session helper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ObservationDto {
    /// Identity of the application that had focus, using the canonical
    /// [`AppKey`] form so the agent can join directly against its catalog
    /// without re-parsing strings.
    pub app_key: AppKey,
    /// Foreground window title, when one could be read. Advisory only.
    pub window_title: Option<String>,
    /// Seconds the user had no input at sampling time, so the agent can
    /// discount idle-but-focused time. Saturating: large values mean "idle".
    pub idle_seconds: u32,
    /// RFC 3339 UTC instant this observation ends at (the sampler collapses
    /// backwards from here). See the clock rules in [`ReportUsageDto`].
    pub observed_at_utc: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LimitTargetDto {
    App {
        #[ts(as = "i32")]
        id: i64,
    },
    Category {
        #[ts(as = "i32")]
        id: i64,
    },
    Total,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DaySummaryDto {
    pub day: DayKey,
    #[ts(as = "i32")]
    pub total_seconds: i64,
    /// Per-app usage, already sorted descending by the agent so the UI does no
    /// work on the main thread.
    pub apps: Vec<UsageRowDto>,
    /// Per-primary-category usage. Sums to `total_seconds`; tags are excluded
    /// here precisely so the chart cannot exceed 100%.
    pub categories: Vec<UsageRowDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UsageRowDto {
    // `as = "i32"`: ts-rs >= 10 renders 64-bit ints as `bigint`, but these
    // values travel as JSON numbers over IPC (all well inside 2^53), so the
    // bindings must say `number`. Same pattern below wherever an i64 crosses.
    #[ts(as = "i32")]
    pub id: i64,
    pub label: String,
    #[ts(as = "i32")]
    pub seconds: i64,
    pub color: Option<String>,
    /// Present when a limit applies, for the progress ring.
    #[ts(as = "Option<i32>")]
    pub limit_seconds: Option<i64>,
    pub blocked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
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
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CatalogDto {
    pub apps: Vec<AppDto>,
    pub categories: Vec<CategoryDto>,
    pub limits: Vec<LimitDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AppDto {
    #[ts(as = "i32")]
    pub id: i64,
    pub key: String,
    pub display_name: String,
    #[ts(as = "i32")]
    pub primary_category: i64,
    #[ts(as = "Vec<i32>")]
    pub tags: Vec<i64>,
    pub user_classified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CategoryDto {
    #[ts(as = "i32")]
    pub id: i64,
    pub slug: String,
    pub name: String,
    /// `limitable` | `block_only` | `never_block`.
    pub kind: String,
    pub color: String,
    pub builtin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LimitDto {
    #[ts(as = "i32")]
    pub id: i64,
    pub target: LimitTargetDto,
    pub default_minutes: u32,
    pub weekday_minutes: [Option<u32>; 7],
    pub enabled: bool,
}

/// A currently-blocked app, as reported to the overlay owner.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BlockedAppDto {
    #[ts(as = "i32")]
    pub app_id: i64,
    pub label: String,
    /// Canonical `kind:value` app key, so the session helper can match the
    /// focused window to the blocked app.
    pub app_key: String,
}

/// The payload of `Response::BlockedApps`. Wrapped in a struct (not a bare
/// `Vec`) because `Response` is internally tagged: Serde cannot tag a newtype
/// variant that is a sequence.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BlockedAppsDto {
    pub blocked: Vec<BlockedAppDto>,
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
        // These strings are wire-visible (the UI and session helper switch on
        // them), so they may only change deliberately.
        let cases: &[(ErrorCode, &str)] = &[
            (ErrorCode::BadPin, "\"bad_pin\""),
            (ErrorCode::CooldownActive, "\"cooldown_active\""),
            (ErrorCode::StrictMode, "\"strict_mode\""),
            (ErrorCode::NotLimitable, "\"not_limitable\""),
            (ErrorCode::NotFound, "\"not_found\""),
            (ErrorCode::BadRequest, "\"bad_request\""),
            (ErrorCode::Internal, "\"internal\""),
        ];
        for (code, expected) in cases {
            let json = serde_json::to_string(&Response::Error {
                code: *code,
                message: "nope".into(),
            })
            .expect("serialise");
            assert!(
                json.contains(expected),
                "expected {expected} in payload for {code:?}: {json}"
            );
        }
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

    #[test]
    fn blocked_apps_response_serialises_with_the_type_tag() {
        // Regression: `Response` is internally tagged, so the payload must be a
        // struct (wrapping the Vec), not a bare sequence.
        let response = Response::BlockedApps(BlockedAppsDto {
            blocked: vec![BlockedAppDto {
                app_id: 6,
                label: "Elden Ring".into(),
                app_key: "win-exe:eldenring.exe".into(),
            }],
        });
        let json = serde_json::to_string(&response).expect("serialise");
        assert!(
            json.contains("\"type\":\"blocked_apps\""),
            "tag missing: {json}"
        );
        assert!(json.contains("Elden Ring"));
    }

    /// A representative batch, as the session helper would send it.
    fn sample_report() -> ReportUsageDto {
        ReportUsageDto {
            observations: vec![
                ObservationDto {
                    app_key: AppKey::windows_exe("C:\\Apps\\Game\\game.exe"),
                    window_title: Some("Elden Ring".into()),
                    idle_seconds: 0,
                    observed_at_utc: "2026-08-25T10:00:30Z".into(),
                },
                ObservationDto {
                    app_key: AppKey::WindowsAumid(
                        "Microsoft.MicrosoftEdge_8wekyb3d8bbwe!MSEDGE".into(),
                    ),
                    window_title: None,
                    idle_seconds: 45,
                    observed_at_utc: "2026-08-25T10:01:00Z".into(),
                },
            ],
        }
    }

    #[test]
    fn report_usage_round_trips_through_framing() {
        let mut buf = Vec::new();
        write_message(
            &mut buf,
            &Request::ReportUsage {
                report: sample_report(),
            },
        )
        .expect("write");

        let mut cursor = Cursor::new(buf);
        let decoded: Request = read_message(&mut cursor).expect("read");
        assert!(matches!(
            decoded,
            Request::ReportUsage { report } if report == sample_report()
        ));
    }

    #[test]
    fn report_usage_serialises_with_the_type_tag() {
        let json = serde_json::to_string(&Request::ReportUsage {
            report: sample_report(),
        })
        .expect("serialise");
        assert!(
            json.contains("\"type\":\"report_usage\""),
            "tag missing: {json}"
        );
    }

    #[test]
    fn report_usage_payload_serialises_with_stable_fields() {
        // Wire-visible shape: the agent joins `app_key` against its catalog and
        // buckets on `observed_at_utc`, so field names and the externally
        // tagged `AppKey` form are contract.
        let json = serde_json::to_string(&sample_report()).expect("serialise");
        assert!(json.contains("\"observations\":["));
        assert!(json.contains("\"app_key\":{\"windows_exe\":\"c:\\\\apps\\\\game\\\\game.exe\"}"));
        assert!(json.contains("\"windows_aumid\""));
        assert!(json.contains("\"window_title\":null"));
        assert!(json.contains("\"idle_seconds\":45"));
        assert!(json.contains("\"observed_at_utc\":\"2026-08-25T10:01:00Z\""));

        let ack = serde_json::to_string(&Response::Accepted {
            effective_utc: "2026-08-25T10:01:00Z".into(),
        })
        .expect("serialise");
        assert!(ack.contains("\"type\":\"accepted\""), "tag missing: {ack}");
        assert!(ack.contains("\"effective_utc\""));
    }

    /// Regenerates the committed TypeScript bindings under
    /// `ui/src/types/generated/`. Run via `cargo test -p st-ipc
    /// export_ts_bindings`. Uses `export_all_to` (not `#[ts(export)]`) so the
    /// output location is pinned to the repo instead of depending on
    /// `TS_RS_EXPORT_DIR` or the working directory.
    #[test]
    fn export_ts_bindings() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/types/generated");
        std::fs::create_dir_all(&dir).expect("create generated dir");
        // Export both protocol roots; every consumed DTO (and its transitive
        // dependencies in st-core) is reachable from one of them.
        Response::export_all_to(&dir).expect("export response bindings");
        Request::export_all_to(&dir).expect("export request bindings");
    }
}
