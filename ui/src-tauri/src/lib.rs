//! Tauri host process for the Screentime UI.
//!
//! Every command exposed here is a thin adapter over an IPC call to the
//! `screentime-agent`. Do not put business logic in this crate: the agent is
//! the source of truth, and duplicating rules in the UI process is how they
//! stop agreeing.

mod ipc_client;

use serde::Serialize;
use st_ipc::Response;

#[derive(Debug, Clone, Serialize)]
pub struct AgentStatus {
    pub version: String,
    pub agent_connected: bool,
    pub tracker_backend: String,
    pub filter_backend: String,
    pub tracking_available: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySummary {
    pub day: i32,
    pub total_seconds: i64,
    pub apps: Vec<UsageRow>,
    pub categories: Vec<UsageRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    pub id: i64,
    pub label: String,
    pub seconds: i64,
    pub color: Option<String>,
}

impl From<st_ipc::DaySummaryDto> for DaySummary {
    fn from(dto: st_ipc::DaySummaryDto) -> Self {
        let map = |row: st_ipc::UsageRowDto| UsageRow {
            id: row.id,
            label: row.label,
            seconds: row.seconds,
            color: row.color,
        };
        Self {
            day: dto.day.0,
            total_seconds: dto.total_seconds,
            apps: dto.apps.into_iter().map(map).collect(),
            categories: dto.categories.into_iter().map(map).collect(),
        }
    }
}

/// Snapshot of what the UI should show in its header.
///
/// This is intentionally a *description* of the current situation rather than a
/// list of individually queryable fields: the header must render a coherent
/// state (either "connected, tracking on Win32" *or* "disconnected") and
/// splitting it across several commands invites the two halves to disagree.
///
/// When the agent is not running, the command still succeeds but reports
/// `agent_connected: false` so the frontend renders a graceful banner instead
/// of an error state.
#[tauri::command]
fn get_status() -> AgentStatus {
    match ipc_client::request(st_ipc::Request::Status) {
        Ok(Response::Status(dto)) => AgentStatus {
            version: dto.agent_version,
            agent_connected: true,
            tracker_backend: dto.tracker_backend,
            filter_backend: dto.filter_backend,
            tracking_available: dto.tracking_available,
        },
        Ok(_) | Err(_) => AgentStatus {
            version: env!("CARGO_PKG_VERSION").to_string(),
            agent_connected: false,
            tracker_backend: if cfg!(windows) { "win32" } else { "x11-stub" }.into(),
            filter_backend: if cfg!(windows) {
                "windows-hosts"
            } else {
                "linux-stub"
            }
            .into(),
            tracking_available: false,
        },
    }
}

/// Dashboard payload for one local day, fetched from the agent.
#[tauri::command]
fn get_day_summary(day: i32) -> Result<DaySummary, String> {
    match ipc_client::request(st_ipc::Request::DaySummary {
        day: st_core::daykey::DayKey(day),
    }) {
        Ok(Response::DaySummary(dto)) => Ok(dto.into()),
        Ok(Response::Error { code, message }) => {
            Err(format!("agent refused ({code:?}): {message}"))
        }
        Ok(_) => Err("unexpected agent response".into()),
        Err(e) => Err(format!("agent unreachable: {e}")),
    }
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![get_status, get_day_summary])
        .run(tauri::generate_context!())
        .expect("failed to launch Tauri application");
}
