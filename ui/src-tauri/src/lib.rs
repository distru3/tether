//! Tauri host process for the Screentime UI.
//!
//! Every command exposed here is a thin adapter over an IPC call to the
//! `screentime-agent`. Do not put business logic in this crate: the agent is
//! the source of truth, and duplicating rules in the UI process is how they
//! stop agreeing.

mod ipc_client;

use serde::{Deserialize, Serialize};
use st_ipc::Response;

#[derive(Debug, Clone, Serialize)]
pub struct AgentStatus {
    pub version: String,
    pub agent_connected: bool,
    pub tracker_backend: String,
    pub filter_backend: String,
    pub tracking_available: bool,
    pub pin_configured: bool,
    pub strict_mode: bool,
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
    pub limit_seconds: Option<i64>,
    pub blocked: bool,
}

impl From<st_ipc::DaySummaryDto> for DaySummary {
    fn from(dto: st_ipc::DaySummaryDto) -> Self {
        let map = |row: st_ipc::UsageRowDto| UsageRow {
            id: row.id,
            label: row.label,
            seconds: row.seconds,
            color: row.color,
            limit_seconds: row.limit_seconds,
            blocked: row.blocked,
        };
        Self {
            day: dto.day.0,
            total_seconds: dto.total_seconds,
            apps: dto.apps.into_iter().map(map).collect(),
            categories: dto.categories.into_iter().map(map).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub apps: Vec<AppInfo>,
    pub categories: Vec<CategoryInfo>,
    pub limits: Vec<LimitInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub id: i64,
    pub key: String,
    pub display_name: String,
    pub primary_category: i64,
    pub tags: Vec<i64>,
    pub user_classified: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInfo {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub kind: String,
    pub color: String,
    pub builtin: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitInfo {
    pub id: i64,
    pub target: LimitTarget,
    pub default_minutes: u32,
    pub weekday_minutes: [Option<u32>; 7],
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LimitTarget {
    App { id: i64 },
    Category { id: i64 },
    Total,
}

impl From<st_ipc::LimitTargetDto> for LimitTarget {
    fn from(dto: st_ipc::LimitTargetDto) -> Self {
        match dto {
            st_ipc::LimitTargetDto::App { id } => LimitTarget::App { id },
            st_ipc::LimitTargetDto::Category { id } => LimitTarget::Category { id },
            st_ipc::LimitTargetDto::Total => LimitTarget::Total,
        }
    }
}

impl From<&LimitTarget> for st_ipc::LimitTargetDto {
    fn from(t: &LimitTarget) -> Self {
        match t {
            LimitTarget::App { id } => st_ipc::LimitTargetDto::App { id: *id },
            LimitTarget::Category { id } => st_ipc::LimitTargetDto::Category { id: *id },
            LimitTarget::Total => st_ipc::LimitTargetDto::Total,
        }
    }
}

impl From<st_ipc::CatalogDto> for Catalog {
    fn from(dto: st_ipc::CatalogDto) -> Self {
        Self {
            apps: dto
                .apps
                .into_iter()
                .map(|a| AppInfo {
                    id: a.id,
                    key: a.key,
                    display_name: a.display_name,
                    primary_category: a.primary_category,
                    tags: a.tags,
                    user_classified: a.user_classified,
                })
                .collect(),
            categories: dto
                .categories
                .into_iter()
                .map(|c| CategoryInfo {
                    id: c.id,
                    slug: c.slug,
                    name: c.name,
                    kind: c.kind,
                    color: c.color,
                    builtin: c.builtin,
                })
                .collect(),
            limits: dto
                .limits
                .into_iter()
                .map(|l| LimitInfo {
                    id: l.id,
                    target: l.target.into(),
                    default_minutes: l.default_minutes,
                    weekday_minutes: l.weekday_minutes,
                    enabled: l.enabled,
                })
                .collect(),
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
            pin_configured: dto.pin_configured,
            strict_mode: dto.strict_mode,
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
            pin_configured: false,
            strict_mode: false,
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

/// Everything the limit editor needs: apps, categories and current limits.
#[tauri::command]
fn get_catalog() -> Result<Catalog, String> {
    match ipc_client::request(st_ipc::Request::Catalog) {
        Ok(Response::Catalog(dto)) => Ok(dto.into()),
        Ok(Response::Error { code, message }) => {
            Err(format!("agent refused ({code:?}): {message}"))
        }
        Ok(_) => Err("unexpected agent response".into()),
        Err(e) => Err(format!("agent unreachable: {e}")),
    }
}

#[tauri::command]
fn set_pin(new_pin: String, current_pin: Option<String>) -> Result<String, String> {
    match ipc_client::request(st_ipc::Request::SetPin {
        new_pin,
        current_pin,
    }) {
        Ok(Response::Accepted { effective_utc }) => Ok(effective_utc),
        Ok(Response::Error { code, message }) => {
            Err(format!("agent refused ({code:?}): {message}"))
        }
        Ok(_) => Err("unexpected agent response".into()),
        Err(e) => Err(format!("agent unreachable: {e}")),
    }
}

#[tauri::command]
fn set_limit(
    target: LimitTarget,
    default_minutes: u32,
    enabled: bool,
    pin: String,
) -> Result<String, String> {
    match ipc_client::request(st_ipc::Request::SetLimit {
        target: (&target).into(),
        default_minutes,
        weekday_minutes: [None; 7],
        enabled,
        pin,
    }) {
        Ok(Response::Accepted { effective_utc }) => Ok(effective_utc),
        Ok(Response::Error { code, message }) => {
            Err(format!("agent refused ({code:?}): {message}"))
        }
        Ok(_) => Err("unexpected agent response".into()),
        Err(e) => Err(format!("agent unreachable: {e}")),
    }
}

#[tauri::command]
fn delete_limit(target: LimitTarget, pin: String) -> Result<String, String> {
    match ipc_client::request(st_ipc::Request::DeleteLimit {
        target: (&target).into(),
        pin,
    }) {
        Ok(Response::Accepted { effective_utc }) => Ok(effective_utc),
        Ok(Response::Error { code, message }) => {
            Err(format!("agent refused ({code:?}): {message}"))
        }
        Ok(_) => Err("unexpected agent response".into()),
        Err(e) => Err(format!("agent unreachable: {e}")),
    }
}

#[tauri::command]
fn grant_override(target: LimitTarget, seconds: i64, pin: String) -> Result<(), String> {
    match ipc_client::request(st_ipc::Request::GrantOverride {
        target: (&target).into(),
        seconds,
        pin,
    }) {
        Ok(Response::Accepted { .. }) => Ok(()),
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
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_day_summary,
            get_catalog,
            set_pin,
            set_limit,
            delete_limit,
            grant_override
        ])
        .run(tauri::generate_context!())
        .expect("failed to launch Tauri application");
}
