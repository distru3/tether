//! IPC server: answers the UI's requests over a local named pipe.
//!
//! One request per connection, matching the transport's design: each UI command
//! opens a connection, sends one frame, reads one response and closes. The
//! server thread blocks on `accept`, so it needs no multiplexing and cannot be
//! pinned by a slow client for longer than one command.
//!
//! M1 handles the read-only surface (`Ping`, `Status`). Everything that would
//! change enforcement state is refused; that surface lands with the limits
//! engine in M2.

use std::sync::{Arc, Mutex};

use st_core::daykey::DayKey;
use st_ipc::{transport, Request, Response, StatusDto, UsageRowDto};
use st_storage::Db;

/// The name of the agent's pipe. The UI and any future session helper connect
/// to `\\.\pipe\{PIPE_NAME}`.
pub const PIPE_NAME: &str = "screentime";

/// Static facts about the agent the UI reports. Extracted once at startup so
/// the IPC thread does not need to touch the platform backends, which are not
/// `Sync` and live on the sampler's thread.
#[derive(Debug, Clone)]
pub struct StatusInfo {
    pub agent_version: String,
    pub tracker_backend: String,
    pub enforcement_backend: String,
    pub filter_backend: String,
    pub tracking_available: bool,
    pub blocks_encrypted_dns: bool,
}

/// Run the IPC server on a background thread. Returns the join handle.
///
/// The thread exits only when the process is torn down; in M1 there is no
/// graceful stop to coordinate.
pub fn spawn(db: Arc<Mutex<Db>>, status: StatusInfo) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("ipc-server".into())
        .spawn(move || loop {
            match transport::server_accept(PIPE_NAME) {
                Ok(mut stream) => match st_ipc::read_message::<_, Request>(&mut stream) {
                    Ok(request) => {
                        let response = handle(&db, &status, request);
                        if let Err(e) = st_ipc::write_message(&mut stream, &response) {
                            tracing::warn!(error = %e, "failed to write IPC response");
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "malformed IPC request"),
                },
                Err(e) => {
                    tracing::error!(error = %e, "IPC accept failed");
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
        })
        .expect("spawning ipc-server thread")
}

fn handle(db: &Mutex<Db>, status: &StatusInfo, request: Request) -> Response {
    match request {
        Request::Ping => Response::Pong,
        Request::Status => status_response(status),
        Request::DaySummary { day } => day_summary(db, day),
        // Everything that mutates enforcement state is refused until M2.
        Request::Categorize { .. }
        | Request::SetLimit { .. }
        | Request::GrantOverride { .. }
        | Request::ReportUsage { .. } => Response::Error {
            code: st_ipc::ErrorCode::Internal,
            message: "not implemented until M2".into(),
        },
    }
}

fn status_response(status: &StatusInfo) -> Response {
    Response::Status(StatusDto {
        agent_version: status.agent_version.clone(),
        tracker_backend: status.tracker_backend.clone(),
        enforcement_backend: status.enforcement_backend.clone(),
        filter_backend: status.filter_backend.clone(),
        tracking_available: status.tracking_available,
        blocks_encrypted_dns: status.blocks_encrypted_dns,
        strict_mode: false,
        pin_configured: false,
    })
}

fn day_summary(db: &Mutex<Db>, day: DayKey) -> Response {
    let summary = match db.lock() {
        Ok(db) => db.day_summary(day),
        Err(_) => {
            return Response::Error {
                code: st_ipc::ErrorCode::Internal,
                message: "database lock poisoned".into(),
            }
        }
    };
    match summary {
        Ok(summary) => Response::DaySummary(st_ipc::DaySummaryDto {
            day: summary.day,
            total_seconds: summary.total_seconds,
            apps: summary
                .apps
                .into_iter()
                .map(|a| UsageRowDto {
                    id: a.id,
                    label: a.label,
                    seconds: a.seconds,
                    color: Some(a.category_color),
                    limit_seconds: None,
                    blocked: false,
                })
                .collect(),
            categories: summary
                .categories
                .into_iter()
                .map(|c| UsageRowDto {
                    id: c.id,
                    label: c.name,
                    seconds: c.seconds,
                    color: Some(c.color),
                    limit_seconds: None,
                    blocked: false,
                })
                .collect(),
        }),
        Err(e) => Response::Error {
            code: st_ipc::ErrorCode::Internal,
            message: format!("dashboard query failed: {e}"),
        },
    }
}
