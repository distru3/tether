//! IPC server: answers the UI's requests over a local named pipe.
//!
//! One request per connection, matching the transport's design: each UI command
//! opens a connection, sends one frame, reads one response and closes. The
//! server thread blocks on `accept`, so it needs no multiplexing and cannot be
//! pinned by a slow client for longer than one command.
//!
//! M1 handled the read-only surface. M2 adds the mutating surface: limits,
//! PIN, overrides and categorisation. Anything that loosens enforcement is
//! PIN-gated (unless no PIN is configured yet) and subject to the anti-impulse
//! cooldown; tightening applies immediately.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use st_core::category::CategoryKind;
use st_core::daykey::DayKey;
use st_core::limits::LimitTarget;
use st_core::model::{AppKey, SubjectRef};
use st_core::pin::{hash_pin, verify_pin};
use st_core::platform::ProcessController;
use st_ipc::{transport, ErrorCode, LimitTargetDto, Request, Response, StatusDto, UsageRowDto};
use st_storage::{Db, LimitRow};

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

/// Policy knobs read from the database. Captured at startup; M4 makes these
/// live-editable and reloadable.
#[derive(Debug, Clone)]
pub struct Policy {
    /// Hours before a loosened limit takes effect.
    pub limit_cooldown_hours: i64,
    /// When true, "+15 minutes" overrides are refused outright.
    pub strict_mode: bool,
}

/// A limit edit as sent by the UI, grouped so the handler stays readable.
#[derive(Debug, Clone)]
struct LimitSpec {
    target: LimitTargetDto,
    default_minutes: u32,
    weekday_minutes: [Option<u32>; 7],
    enabled: bool,
    pin: String,
}

/// Run the IPC server on a background thread. Returns the join handle.
///
/// The thread exits only when the process is torn down; in M1 there is no
/// graceful stop to coordinate.
pub fn spawn(
    db: Arc<Mutex<Db>>,
    status: StatusInfo,
    policy: Policy,
    processes: Arc<Mutex<Box<dyn ProcessController>>>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("ipc-server".into())
        .spawn(move || loop {
            match transport::server_accept(PIPE_NAME) {
                Ok(mut stream) => match st_ipc::read_message::<_, Request>(&mut stream) {
                    Ok(request) => {
                        let response = handle(&db, &status, &policy, &processes, request);
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

fn handle(
    db: &Mutex<Db>,
    status: &StatusInfo,
    policy: &Policy,
    processes: &Mutex<Box<dyn ProcessController>>,
    request: Request,
) -> Response {
    let now = Utc::now();
    match request {
        Request::Ping => Response::Pong,
        Request::Status => status_response(db, status, policy),
        Request::DaySummary { day } => day_summary(db, day),
        Request::Catalog => catalog(db),
        Request::BlockedApps => blocked_apps(db),
        Request::CloseApps { app_id, pin } => close_apps(db, processes, app_id, &pin),
        Request::SetPin {
            new_pin,
            current_pin,
        } => set_pin(db, &new_pin, current_pin.as_deref()),
        Request::SetLimit {
            target,
            default_minutes,
            weekday_minutes,
            enabled,
            pin,
        } => set_limit(
            db,
            policy,
            LimitSpec {
                target,
                default_minutes,
                weekday_minutes,
                enabled,
                pin,
            },
            now,
        ),
        Request::DeleteLimit { target, pin } => delete_limit(db, policy, target, &pin, now),
        Request::GrantOverride {
            target,
            seconds,
            pin,
        } => grant_override(db, policy, target, seconds, &pin, now),
        Request::Categorize {
            app_id,
            primary,
            tags,
        } => categorize(db, app_id, primary, &tags),
        Request::ReportUsage { .. } => Response::Error {
            code: ErrorCode::Internal,
            message: "session-helper reporting lands with the session binary".into(),
        },
    }
}

fn status_response(db: &Mutex<Db>, status: &StatusInfo, policy: &Policy) -> Response {
    let pin_configured = db
        .lock()
        .ok()
        .and_then(|db| db.pin_hash().ok().flatten())
        .is_some();
    Response::Status(StatusDto {
        agent_version: status.agent_version.clone(),
        tracker_backend: status.tracker_backend.clone(),
        enforcement_backend: status.enforcement_backend.clone(),
        filter_backend: status.filter_backend.clone(),
        tracking_available: status.tracking_available,
        blocks_encrypted_dns: status.blocks_encrypted_dns,
        strict_mode: policy.strict_mode,
        pin_configured,
    })
}

fn day_summary(db: &Mutex<Db>, day: DayKey) -> Response {
    let db = match db.lock() {
        Ok(db) => db,
        Err(_) => {
            return Response::Error {
                code: ErrorCode::Internal,
                message: "database lock poisoned".into(),
            }
        }
    };
    let summary = match db.day_summary(day) {
        Ok(summary) => summary,
        Err(e) => {
            return Response::Error {
                code: ErrorCode::Internal,
                message: format!("dashboard query failed: {e}"),
            }
        }
    };

    // Which apps are currently frozen, for honest blocked flags.
    let blocked_apps: std::collections::HashSet<i64> = db
        .blocked_subjects()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|(subject, _)| match subject {
            st_core::model::SubjectRef::App(id) => Some(id),
            st_core::model::SubjectRef::Site(_) => None,
        })
        .collect();

    // Per-app allowed seconds from limits, for progress rings.
    let limits = db.load_limits().unwrap_or_default();
    let limit_secs_for = |app_id: i64| -> Option<i64> {
        limits
            .iter()
            .find(|l| matches!(l.target, st_core::limits::LimitTarget::App(id) if id == app_id))
            .map(|l| i64::from(l.default_minutes) * 60)
    };

    Response::DaySummary(st_ipc::DaySummaryDto {
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
                limit_seconds: limit_secs_for(a.id),
                blocked: blocked_apps.contains(&a.id),
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
    })
}

fn catalog(db: &Mutex<Db>) -> Response {
    let db = match db.lock() {
        Ok(db) => db,
        Err(_) => {
            return Response::Error {
                code: ErrorCode::Internal,
                message: "database lock poisoned".into(),
            }
        }
    };
    let (apps, categories, limits) =
        match (db.list_apps(), db.list_categories(), db.list_limit_rows()) {
            (Ok(apps), Ok(categories), Ok(limits)) => (apps, categories, limits),
            (Err(e), _, _) => return error_internal(format!("list apps: {e}")),
            (_, Err(e), _) => return error_internal(format!("list categories: {e}")),
            (_, _, Err(e)) => return error_internal(format!("list limits: {e}")),
        };

    Response::Catalog(st_ipc::CatalogDto {
        apps: apps
            .into_iter()
            .map(|a| st_ipc::AppDto {
                id: a.id,
                key: a.key.to_db_string(),
                display_name: a.display_name,
                primary_category: a.primary_category,
                tags: a.tags,
                user_classified: a.user_classified,
            })
            .collect(),
        categories: categories
            .into_iter()
            .map(|c| st_ipc::CategoryDto {
                id: c.id,
                slug: c.slug,
                name: c.name,
                kind: match c.kind {
                    CategoryKind::Limitable => "limitable",
                    CategoryKind::BlockOnly => "block_only",
                    CategoryKind::NeverBlock => "never_block",
                }
                .to_string(),
                color: c.color,
                builtin: c.builtin,
            })
            .collect(),
        limits: limits.iter().filter_map(limit_to_dto).collect(),
    })
}

/// Currently-blocked apps, for the overlay owner. Joins `block_state` with the
/// app rows to hand back a label and key the session helper can match.
fn blocked_apps(db: &Mutex<Db>) -> Response {
    let db = match db.lock() {
        Ok(db) => db,
        Err(_) => return error_internal("database lock poisoned".into()),
    };
    let blocked = match db.blocked_subjects() {
        Ok(b) => b,
        Err(e) => return error_internal(format!("list blocks: {e}")),
    };
    let mut out = Vec::new();
    for (subject, _reason) in blocked {
        let SubjectRef::App(app_id) = subject else {
            continue;
        };
        let Some(record) = db.app_record(app_id).ok().flatten() else {
            continue;
        };
        out.push(st_ipc::BlockedAppDto {
            app_id,
            label: record.display_name,
            app_key: record.key.to_db_string(),
        });
    }
    Response::BlockedApps(out)
}

/// "Quit" from the block overlay: terminate the app's process tree. This is a
/// deliberate user action (not the removed auto-freeze), so it is PIN-gated
/// like every other mutating request.
fn close_apps(
    db: &Mutex<Db>,
    processes: &Mutex<Box<dyn ProcessController>>,
    app_id: i64,
    pin: &str,
) -> Response {
    let db = match db.lock() {
        Ok(db) => db,
        Err(_) => return error_internal("database lock poisoned".into()),
    };
    if !pin_ok(&db, pin) {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "PIN required".into(),
        };
    }
    let Some(record) = db.app_record(app_id).ok().flatten() else {
        return Response::Error {
            code: ErrorCode::NotFound,
            message: format!("app {app_id} not found"),
        };
    };
    let key: AppKey = record.key;
    let Ok(mut processes) = processes.lock() else {
        return error_internal("process controller lock poisoned".into());
    };
    let pids = match processes.find_processes(&key) {
        Ok(pids) => pids,
        Err(e) => return error_internal(format!("find processes: {e}")),
    };
    for pid in pids {
        if let Err(e) = processes.terminate(pid) {
            tracing::warn!(pid, error = %e, "terminate failed (process may have exited)");
        }
    }
    let _ = db.clear_block(SubjectRef::App(app_id));
    accepted(Utc::now())
}

fn set_pin(db: &Mutex<Db>, new_pin: &str, current_pin: Option<&str>) -> Response {
    if new_pin.is_empty() {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "PIN cannot be empty".into(),
        };
    }
    let db = match db.lock() {
        Ok(db) => db,
        Err(_) => return error_internal("database lock poisoned".into()),
    };
    match db.pin_hash() {
        Ok(Some(stored)) => {
            let current = current_pin.unwrap_or("");
            if !verify_pin(current, &stored) {
                return Response::Error {
                    code: ErrorCode::BadPin,
                    message: "current PIN does not match".into(),
                };
            }
        }
        Ok(None) => {}
        Err(e) => return error_internal(format!("read pin: {e}")),
    }
    match hash_pin(new_pin) {
        Ok(hash) => match db.set_pin_hash(&hash) {
            Ok(()) => accepted(Utc::now()),
            Err(e) => error_internal(format!("store pin: {e}")),
        },
        Err(e) => error_internal(format!("hash pin: {e}")),
    }
}

fn set_limit(db: &Mutex<Db>, policy: &Policy, spec: LimitSpec, now: DateTime<Utc>) -> Response {
    let db = match db.lock() {
        Ok(db) => db,
        Err(_) => return error_internal("database lock poisoned".into()),
    };
    if !pin_ok(&db, &spec.pin) {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "PIN required".into(),
        };
    }
    let Some(target) = dto_to_target(&spec.target) else {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "unknown target".into(),
        };
    };
    if let Err(code) = validate_target(&db, &target) {
        return Response::Error {
            code,
            message: "target cannot carry a time limit".into(),
        };
    }

    let existing = db
        .load_limits()
        .ok()
        .and_then(|ls| ls.into_iter().find(|l| l.target == target));
    let effective = match existing {
        // Tightening (or a brand-new limit) applies immediately.
        Some(existing)
            if !is_loosening(
                &existing,
                spec.default_minutes,
                spec.weekday_minutes,
                spec.enabled,
            ) =>
        {
            now
        }
        Some(_) => now + chrono::Duration::hours(policy.limit_cooldown_hours.max(0)),
        None => now,
    };

    let result = if effective > now {
        db.queue_pending_update(
            &target,
            spec.default_minutes,
            spec.weekday_minutes,
            spec.enabled,
            effective,
        )
    } else {
        db.upsert_limit(
            &st_core::limits::Limit {
                id: 0,
                target,
                default_minutes: spec.default_minutes,
                weekday_minutes: spec.weekday_minutes,
                enabled: spec.enabled,
            },
            now,
        )
    };
    match result {
        Ok(()) => Response::Accepted {
            effective_utc: effective.to_rfc3339(),
        },
        Err(e) => error_internal(format!("set limit: {e}")),
    }
}

fn delete_limit(
    db: &Mutex<Db>,
    policy: &Policy,
    target: LimitTargetDto,
    pin: &str,
    now: DateTime<Utc>,
) -> Response {
    let db = match db.lock() {
        Ok(db) => db,
        Err(_) => return error_internal("database lock poisoned".into()),
    };
    if !pin_ok(&db, pin) {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "PIN required".into(),
        };
    }
    let Some(target) = dto_to_target(&target) else {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "unknown target".into(),
        };
    };
    // Deleting is always a loosening action: cooldown applies.
    let effective = now + chrono::Duration::hours(policy.limit_cooldown_hours.max(0));
    let result = if effective > now {
        db.queue_pending_delete(&target, effective)
    } else {
        db.delete_limit_by_target(&target)
    };
    match result {
        Ok(()) => Response::Accepted {
            effective_utc: effective.to_rfc3339(),
        },
        Err(e) => error_internal(format!("delete limit: {e}")),
    }
}

fn grant_override(
    db: &Mutex<Db>,
    policy: &Policy,
    target: LimitTargetDto,
    seconds: i64,
    pin: &str,
    now: DateTime<Utc>,
) -> Response {
    let db = match db.lock() {
        Ok(db) => db,
        Err(_) => return error_internal("database lock poisoned".into()),
    };
    if policy.strict_mode {
        return Response::Error {
            code: ErrorCode::StrictMode,
            message: "overrides are disabled in strict mode".into(),
        };
    }
    if !pin_ok(&db, pin) {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "PIN required".into(),
        };
    }
    let Some(target) = dto_to_target(&target) else {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "unknown target".into(),
        };
    };
    let seconds = seconds.clamp(1, 24 * 3600);
    let day = DayKey::from_utc(now, 0, 0);
    match db.grant_override(&target, day, seconds, now, Some("user override")) {
        Ok(()) => accepted(now),
        Err(e) => error_internal(format!("grant override: {e}")),
    }
}

fn categorize(db: &Mutex<Db>, app_id: i64, primary: i64, tags: &[i64]) -> Response {
    let mut db = match db.lock() {
        Ok(db) => db,
        Err(_) => return error_internal("database lock poisoned".into()),
    };
    match db.set_app_categories(app_id, primary, tags, true) {
        Ok(()) => accepted(Utc::now()),
        Err(e) => error_internal(format!("categorise: {e}")),
    }
}

/// A limit is "loosening" if any dimension went up or it was disabled. Mixed
/// changes are treated conservatively as loosening.
fn is_loosening(
    existing: &st_core::limits::Limit,
    default_minutes: u32,
    weekday_minutes: [Option<u32>; 7],
    enabled: bool,
) -> bool {
    if !enabled && existing.enabled {
        return true;
    }
    if default_minutes > existing.default_minutes {
        return true;
    }
    weekday_minutes
        .iter()
        .zip(existing.weekday_minutes.iter())
        .any(|(new, old)| match (new, old) {
            (Some(n), Some(o)) => n > o,
            (Some(_), None) => true,
            (None, Some(_)) => true,
            (None, None) => false,
        })
}

/// Check that the target can carry a time limit at all.
fn validate_target(db: &Db, target: &LimitTarget) -> std::result::Result<(), ErrorCode> {
    match target {
        LimitTarget::Total => Ok(()),
        LimitTarget::App(_) => Ok(()),
        LimitTarget::Category(id) => {
            let kind = db.category_kind(*id).ok().flatten();
            match kind {
                Some(CategoryKind::Limitable) => Ok(()),
                Some(CategoryKind::BlockOnly) | Some(CategoryKind::NeverBlock) | None => {
                    Err(ErrorCode::NotLimitable)
                }
            }
        }
    }
}

fn pin_ok(db: &Db, pin: &str) -> bool {
    match db.pin_hash() {
        Ok(Some(stored)) => verify_pin(pin, &stored),
        // No PIN configured yet: limit changes are allowed without one (M2
        // decision, so the app can be dogfooded before the user sets a PIN).
        Ok(None) => true,
        Err(_) => false,
    }
}

fn dto_to_target(dto: &LimitTargetDto) -> Option<LimitTarget> {
    match dto {
        LimitTargetDto::App { id } => Some(LimitTarget::App(*id)),
        LimitTargetDto::Category { id } => Some(LimitTarget::Category(*id)),
        LimitTargetDto::Total => Some(LimitTarget::Total),
    }
}

fn limit_to_dto(row: &LimitRow) -> Option<st_ipc::LimitDto> {
    let limit = row.to_limit()?;
    let target = match limit.target {
        LimitTarget::App(id) => LimitTargetDto::App { id },
        LimitTarget::Category(id) => LimitTargetDto::Category { id },
        LimitTarget::Total => LimitTargetDto::Total,
    };
    Some(st_ipc::LimitDto {
        id: limit.id,
        target,
        default_minutes: limit.default_minutes,
        weekday_minutes: limit.weekday_minutes,
        enabled: limit.enabled,
    })
}

fn accepted(effective_utc: DateTime<Utc>) -> Response {
    Response::Accepted {
        effective_utc: effective_utc.to_rfc3339(),
    }
}

fn error_internal(message: String) -> Response {
    Response::Error {
        code: ErrorCode::Internal,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use st_core::limits::Limit;

    fn base() -> Limit {
        Limit::new(1, LimitTarget::Total, 30)
    }

    #[test]
    fn tightening_is_not_loosening() {
        let existing = base();
        assert!(!is_loosening(&existing, 15, [None; 7], true));
        assert!(!is_loosening(&existing, 30, [None; 7], true));
    }

    #[test]
    fn increasing_minutes_is_loosening() {
        let existing = base();
        assert!(is_loosening(&existing, 45, [None; 7], true));
    }

    #[test]
    fn disabling_is_loosening() {
        let existing = base();
        assert!(is_loosening(&existing, 30, [None; 7], false));
    }

    #[test]
    fn raising_a_weekday_override_is_loosening() {
        let existing = base();
        let mut new = [None; 7];
        new[6] = Some(120);
        assert!(is_loosening(&existing, 30, new, true));
    }

    #[test]
    fn lowering_a_weekday_override_is_tightening() {
        let mut existing = base();
        existing.weekday_minutes[6] = Some(120);
        let mut new = [None; 7];
        new[6] = Some(60);
        assert!(!is_loosening(&existing, 30, new, true));
    }

    #[test]
    fn mixed_changes_are_treated_as_loosening() {
        let existing = base();
        let mut new = [None; 7];
        new[0] = Some(15); // tighter on Monday
        new[6] = Some(120); // looser on Sunday
        assert!(is_loosening(&existing, 30, new, true));
    }

    #[test]
    fn pin_gate_accepts_no_pin_before_one_is_set() {
        let db = Db::open_in_memory().expect("db");
        assert!(pin_ok(&db, "anything"));
    }

    #[test]
    fn pin_gate_requires_a_matching_pin_once_set() {
        let db = Db::open_in_memory().expect("db");
        db.set_pin_hash(&hash_pin("1234").expect("hash"))
            .expect("set");
        assert!(pin_ok(&db, "1234"));
        assert!(!pin_ok(&db, "wrong"));
    }
}
