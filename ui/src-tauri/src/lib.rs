//! Tauri host process for the Screentime UI.
//!
//! Every command exposed here is a thin adapter over an IPC call to the
//! `screentime-agent`. Do not put business logic in this crate: the agent is
//! the source of truth, and duplicating rules in the UI process is how they
//! stop agreeing.
//!
//! # Current scope
//!
//! `get_status` returns a stub payload derived from local information. It
//! exists so the frontend has a working `invoke` round-trip; the real
//! implementation opens the IPC connection to the agent, and lands with the
//! rest of the IPC work in M2.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AgentStatus {
    pub version: String,
    pub agent_connected: bool,
    pub tracker_backend: String,
    pub filter_backend: String,
    pub tracking_available: bool,
}

/// Snapshot of what the UI should show in its header.
///
/// This is intentionally a *description* of the current situation rather than a
/// list of individually queryable fields: the header must render a coherent
/// state (either "connected, tracking on Win32" *or* "disconnected") and
/// splitting it across several commands invites the two halves to disagree.
#[tauri::command]
fn get_status() -> AgentStatus {
    AgentStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        agent_connected: false,
        tracker_backend: if cfg!(windows) { "win32" } else { "x11-stub" }.into(),
        filter_backend: if cfg!(windows) {
            "windows-hosts"
        } else {
            "linux-stub"
        }
        .into(),
        tracking_available: cfg!(windows),
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
        .invoke_handler(tauri::generate_handler![get_status])
        .run(tauri::generate_context!())
        .expect("failed to launch Tauri application");
}
