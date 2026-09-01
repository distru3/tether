//! IPC server: answers the UI's requests over a local named pipe.
//!
//! # Concurrency model
//!
//! Thread-per-connection. The accept loop parks when [`MAX_WORKERS`] workers
//! are live, so connection floods cannot exhaust threads or memory, and each
//! worker serves its connection for as long as the client keeps it open —
//! looping read/handle/write until a clean close or a protocol error. Two
//! client shapes are therefore served by one code path:
//!
//! * legacy one-shot clients (one frame, then close), and
//! * persistent clients (many frames per connection, e.g. the session helper
//!   polling `BlockedApps` and streaming `ReportUsage`).
//!
//! A slow *or* idle client pins one worker, never the whole server; the cap
//! bounds worst-case resource use. Shared state (`Db`, the process controller,
//! the runtime facts in [`Live`]) lives behind `Arc`s cloned into each worker.
//! Requests themselves stay sequential per connection — the wire has no
//! correlation ids, so pipelining is not supported by design.
//!
//! M1 handled the read-only surface. M2 adds the mutating surface: limits,
//! PIN, overrides and categorisation. Anything that loosens enforcement is
//! PIN-gated (unless no PIN is configured yet) and subject to the anti-impulse
//! cooldown; tightening applies immediately.
//!
//! # Where time comes from
//!
//! Every timestamp in this file flows from the injected
//! [`Clock`](st_core::clock::Clock), never `Utc::now()`. Tamper semantics (and
//! every handler-level test) depend on wall time being controllable.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use st_core::category::CategoryKind;
use st_core::clock::Clock;
use st_core::daykey::DayKey;
use st_core::limits::{LimitTarget, UsageSnapshot};
use st_core::model::{AppKey, SubjectRef};
use st_core::pin::{generate_recovery_code, hash_pin, normalize_recovery_code, verify_pin};
use st_core::platform::ProcessController;
use st_ipc::{
    transport, ErrorCode, LimitTargetDto, ObservationDto, ReportUsageDto,
    Request, Response, StatusDto, UsageRowDto,
};
use st_storage::{Db, LimitRow};

use crate::locks::{lock_db, lock_recover};
use crate::persist;
use crate::sampler::PendingInterval;

/// Concurrent connections served before the accept loop starts parking. The
/// honest client population is a UI plus a session helper per login session;
/// 32 leaves ample headroom while capping thread/memory use against a hostile
/// or buggy local process opening sockets in a loop.
const MAX_WORKERS: usize = 32;

// -- Report-ingestion tuning knobs. -----------------------------------------

/// An observation timestamped further ahead than this is rejected outright:
/// it would credit future time and land usage on a day that has not happened.
/// Generous enough for modest positive clock skew between machines.
const REPORT_MAX_FUTURE_SECS: i64 = 120;

/// Observations older than this are stale buffer dumps or replays, not live
/// reporting; they are dropped rather than silently reshaping old days.
const REPORT_MAX_AGE_SECS: i64 = 48 * 3600;

/// Past-skew worth warning about (but still crediting) so chronic skew is
/// visible in the logs without discarding data.
const REPORT_SKEW_WARN_SECS: i64 = 600;

/// Maximum gap two consecutive active observations of the same app may have
/// and still count as one continuous focus run. Must exceed the helper's flush
/// period ("a few seconds" per the wire contract) with room for jitter, yet
/// stay far below a task switch.
const CHAIN_GAP_SECS: i64 = 30;

/// How long after the last accepted report `tracking_available` stays true.
/// The helper flushes every few seconds, so a minute of silence means it is
/// gone (crashed, logged out) and the UI should say tracking is degraded.
const RECENT_REPORT_SECS: i64 = 60;

/// Focus used for enforcement older than this is stale: the helper stopped
/// reporting, so the main loop should stop evaluating that app.
const FOCUS_MAX_AGE_SECS: i64 = 30;

/// Cross-batch resend protection: identical `(app_key, observed_at)` pairs
/// within this window are collapsed. Covers the realistic case — a lost reply
/// triggers a resend seconds later. Agent restart clears the window, which is
/// acceptable: resends do not outlive the helper either.
const DEDUPE_TTL_SECS: i64 = 300;

/// Hard bound on the dedupe ring so a hostile reporter cannot grow memory.
const DEDUPE_CAP: usize = 8192;

/// Static facts about the agent the UI reports. Extracted once at startup so
/// IPC threads do not touch the platform backends.
#[derive(Debug, Clone)]
pub struct StatusInfo {
    pub agent_version: String,
    pub tracker_backend: String,
    pub enforcement_backend: String,
    pub filter_backend: String,
    /// Whether the agent is running its own in-process sampling loop (the
    /// development fallback), as opposed to relying on session-helper reports.
    pub self_sampling: bool,
    // Hosts-file filtering cannot intercept DoH/DoT; be honest about it.
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
    /// Minutes after local midnight at which the day rolls over. Overrides
    /// must be attributed to exactly the day the enforcer/sampler compute.
    pub day_start_minutes: i64,
    /// Idle seconds beyond which focused time stops accruing.
    pub idle_threshold_secs: i64,
    /// Whether to show the remaining time HUD on limited apps.
    pub show_hud_overlay: bool,
}

/// Runtime-updated facts shared between IPC workers and the main loop.
///
/// Three small independent mutexes instead of one: handlers touch them one at
/// a time, and nested locking is a deadlock waiting to happen.
pub struct Live {
    last_report: Mutex<Option<DateTime<Utc>>>,
    focus: Mutex<Option<(AppKey, DateTime<Utc>)>>,
    seen_observations: Mutex<Vec<(String, i64)>>,
    open_chains: Mutex<HashMap<String, ChainState>>,
}

impl Default for Live {
    fn default() -> Self {
        Self {
            last_report: Mutex::new(None),
            focus: Mutex::new(None),
            seen_observations: Mutex::new(Vec::new()),
            open_chains: Mutex::new(HashMap::new()),
        }
    }
}

/// One app's open focus run: the instant its latest credited observation ended.
#[derive(Debug, Clone, Copy)]
struct ChainState {
    last: DateTime<Utc>,
}

impl Live {
    fn note_report(&self, now: DateTime<Utc>) {
        *lock_recover(&self.last_report, "last-report stamp") = Some(now);
    }

    fn reported_within(&self, secs: i64, now: DateTime<Utc>) -> bool {
        lock_recover(&self.last_report, "last-report stamp")
            .is_some_and(|t| (now - t).num_seconds() <= secs)
    }

    fn note_focus(&self, key: AppKey, at: DateTime<Utc>) {
        let mut focus = lock_recover(&self.focus, "focus");
        if focus.as_ref().is_none_or(|(_, t)| at >= *t) {
            *focus = Some((key, at));
        }
    }

    /// The most recently observed focused app, if the observation is fresh
    /// enough to act on for enforcement.
    pub(crate) fn focus_key(&self, now: DateTime<Utc>) -> Option<AppKey> {
        lock_recover(&self.focus, "focus")
            .clone()
            .filter(|(_, t)| (now - *t).num_seconds() <= FOCUS_MAX_AGE_SECS)
            .map(|(k, _)| k)
    }
}

/// Everything a connection worker needs. Cloned per worker; every field is an
/// cheap handle onto shared state.
#[derive(Clone)]
pub struct Ctx {
    db: Arc<Mutex<Db>>,
    status: Arc<StatusInfo>,
    policy: Arc<std::sync::RwLock<Policy>>,
    processes: Arc<Mutex<Box<dyn ProcessController>>>,
    clock: Arc<dyn Clock>,
    live: Arc<Live>,
}

/// Slot accounting for the worker cap: a count plus a condvar the accept loop
/// waits on when full.
struct WorkerCap {
    count: Mutex<usize>,
    slot_free: Condvar,
}

impl WorkerCap {
    fn new() -> Self {
        Self {
            count: Mutex::new(0),
            slot_free: Condvar::new(),
        }
    }

    fn acquire(&self) {
        let mut n = lock_recover(&self.count, "worker counter");
        while *n >= MAX_WORKERS {
            n = self
                .slot_free
                .wait(n)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        *n += 1;
    }

    fn release(&self) {
        let mut n = lock_recover(&self.count, "worker counter");
        *n = n.saturating_sub(1);
        self.slot_free.notify_one();
    }
}

/// Returns a worker slot on drop.
///
/// Why a guard instead of calling `release()` after the serve call: if the
/// worker thread panics mid-connection, code after the panic site never runs,
/// and every skipped release permanently shrinks the worker pool. Once enough
/// slots leaked, `acquire` blocked forever, the accept loop stopped creating
/// pipe instances, and the agent went deaf while its process lived on. The
/// guard makes leak-free release unconditional; combined with `catch_unwind`
/// below, one poisoned request can no longer cost the server its ear.
struct SlotGuard(WorkerCapGuardInner);

type WorkerCapGuardInner = Arc<WorkerCap>;

impl SlotGuard {
    fn acquire(cap: &Arc<WorkerCap>) -> Self {
        cap.acquire();
        Self(Arc::clone(cap))
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        self.0.release();
    }
}

/// Handle on the running IPC server.
///
/// Deliberately no `JoinHandle`: the accept loop runs until process teardown
/// (there is no graceful-stop protocol yet), so there is nothing useful to
/// join on.
pub struct IpcServerHandle {
    /// Shared runtime facts the main loop reads (focus for enforcement,
    /// recency for `tracking_available`).
    pub live: Arc<Live>,
}

/// A pipe stream moved into its worker thread.
///
/// `PipeStream` holds a raw Win32 `HANDLE`, which is not `Send` as far as
/// rustc can see — but kernel handles carry no thread affinity, and this
/// wrapper takes *exclusive ownership* when moved, so serving the connection
/// from a different thread than `accept` is sound. Nothing else ever touches
/// the handle after the move.
struct OwnedStream(st_ipc::transport::PipeStream);

// SAFETY: see the type-level comment. Exclusive ownership of a thread-agnostic
// kernel object; no shared state escapes.
unsafe impl Send for OwnedStream {}

/// Run the IPC server: an accept loop plus up to [`MAX_WORKERS`] connection
/// workers. Returns handles to coordinate shutdown and share runtime facts.
pub fn spawn(
    pipe_name: &'static str,
    db: Arc<Mutex<Db>>,
    status: StatusInfo,
    policy: Policy,
    processes: Arc<Mutex<Box<dyn ProcessController>>>,
    clock: Arc<dyn Clock>,
) -> IpcServerHandle {
    let live = Arc::new(Live::default());
    let ctx = Ctx {
        db,
        status: Arc::new(status),
        policy: Arc::new(std::sync::RwLock::new(policy)),
        processes,
        clock,
        live: live.clone(),
    };
    let workers = Arc::new(WorkerCap::new());

    // Detach the accept loop; see `IpcServerHandle` for why there is no join.
    std::thread::Builder::new()
        .name("ipc-server".into())
        .spawn(move || loop {
            // Acquire before accept so a saturated server stops advertising an
            // instance it could not afford to serve; the SlotGuard guarantees
            // the slot comes back even if everything below unwinds.
            let guard = SlotGuard::acquire(&workers);
            match transport::server_accept(pipe_name) {
                Ok(stream) => {
                    let ctx = ctx.clone();
                    let owned = OwnedStream(stream);
                    let spawned = std::thread::Builder::new()
                        .name("ipc-worker".into())
                        .spawn(move || {
                            // A panic in a handler must cost one connection,
                            // never the worker pool or the listening loop.
                            let result = std::panic::catch_unwind(
                                std::panic::AssertUnwindSafe(|| serve_connection(owned, ctx)),
                            );
                            if result.is_err() {
                                tracing::error!(
                                    "IPC connection handler panicked; connection dropped, server continues"
                                );
                            }
                            // guard drops here: slot released unconditionally
                        });
                    if spawned.is_err() {
                        // Thread creation failed (resource exhaustion): the
                        // guard's drop refunds the slot; pause so a tight loop
                        // cannot spin.
                        drop(guard);
                        std::thread::sleep(StdDuration::from_secs(1));
                    }
                }
                Err(e) => {
                    drop(guard);
                    tracing::error!(error = %e, "IPC accept failed");
                    std::thread::sleep(StdDuration::from_secs(1));
                }
            }
        })
        .expect("spawning ipc-server thread");

    IpcServerHandle { live }
}

/// Serve one connection until the peer closes cleanly or breaks protocol.
///
/// Takes [`OwnedStream`] so the moved value is the `Send` wrapper; capturing
/// the inner stream directly would re-expose the raw handle as non-Send
/// (edition-2021 closures capture precise places, not whole bindings).
fn serve_connection(stream: OwnedStream, ctx: Ctx) {
    let OwnedStream(mut stream) = stream;
    loop {
        match st_ipc::read_message::<_, Request>(&mut stream) {
            Ok(request) => {
                let response = handle(&ctx, request);
                if let Err(e) = st_ipc::write_message(&mut stream, &response) {
                    tracing::warn!(error = %e, "failed to write IPC response; closing connection");
                    break;
                }
            }
            // Clean close: the normal end for one-shot clients.
            Err(st_ipc::IpcError::Closed) => break,
            Err(e) => {
                tracing::warn!(error = %e, "malformed IPC frame; closing connection");
                break;
            }
        }
    }
}

fn list_manual_blocks(ctx: &Ctx) -> Response {
    let db = lock_db(&ctx.db);
    match db.list_block_rules() {
        Ok(rows) => {
            let domains = rows
                .into_iter()
                .filter(|r| r.blocklist_id.is_none() && r.category_id.is_none())
                .map(|r| r.domain)
                .collect();
            Response::ManualBlocks { domains }
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to list manual blocks");
            Response::Error {
                code: ErrorCode::Internal,
                message: "Database error".into(),
            }
        }
    }
}

fn add_manual_block(ctx: &Ctx, domain: &str) -> Response {
    let db = lock_db(&ctx.db);
    if let Err(e) = db.add_block_rule(None, None, domain, true, "block") {
        tracing::error!(error = %e, "failed to add manual block");
        return Response::Error {
            code: ErrorCode::Internal,
            message: "Database error".into(),
        };
    }
    Response::Accepted {
        hud: None,
        effective_utc: ctx.clock.now_utc().to_rfc3339(),
    }
}

fn remove_manual_block(ctx: &Ctx, domain: &str, pin: &str) -> Response {
    let db = lock_db(&ctx.db);
    if !pin_ok(&db, pin) {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "PIN required".into(),
        };
    }
    // Find the rule
    let rule_id = match db.list_block_rules() {
        Ok(rows) => rows
            .into_iter()
            .find(|r| r.blocklist_id.is_none() && r.domain == domain)
            .map(|r| r.id),
        Err(_) => None,
    };

    if let Some(id) = rule_id {
        if let Err(e) = db.delete_block_rule(id) {
            tracing::error!(error = %e, "failed to remove manual block");
        }
    }
    Response::Accepted {
        hud: None,
        effective_utc: ctx.clock.now_utc().to_rfc3339(),
    }
}

fn handle(ctx: &Ctx, request: Request) -> Response {
    let now = ctx.clock.now_utc();
    match request {
        Request::Ping => Response::Pong,
        Request::Status => status_response(ctx),
        Request::DaySummary { day } => day_summary(ctx, day),
        Request::WeeklySummary { end_day } => weekly_summary(ctx, end_day),
        Request::Catalog => catalog(ctx),
        Request::BlockedApps => blocked_apps(ctx),
        Request::CloseApps { app_id, pin } => close_apps(ctx, app_id, &pin),
        Request::SetPin {
            new_pin,
            current_pin,
        } => set_pin(ctx, &new_pin, current_pin.as_deref()),
        Request::RecoverPin {
            recovery_code,
            new_pin,
        } => recover_pin(ctx, &recovery_code, &new_pin),
        Request::RemovePin { credential } => remove_pin(ctx, &credential),
        Request::SetSetting { ref key, ref value } => {
            let db_guard = lock_db(&ctx.db);
            if let Err(e) = db_guard.conn().execute("INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", (key, value)) {
                tracing::error!(error = %e, "setting save failed");
            } else {
                let mut p = ctx.policy.write().unwrap();
                match key.as_str() {
                    "limit_cooldown_hours" => if let Ok(v) = value.parse() { p.limit_cooldown_hours = v; }
                    "strict_mode" => p.strict_mode = value == "true",
                    "day_start_minutes" => if let Ok(v) = value.parse() { p.day_start_minutes = v; }
                    "idle_threshold_secs" => if let Ok(v) = value.parse() { p.idle_threshold_secs = v; }
                    "show_hud_overlay" => p.show_hud_overlay = value != "false",
                    _ => {}
                }
            }
            Response::Accepted {
                effective_utc: now.to_rfc3339(),
                hud: None,
            }
        }
        Request::SetLimit {
            target,
            default_minutes,
            weekday_minutes,
            enabled,
            pin,
        } => set_limit(
            ctx,
            LimitSpec {
                target,
                default_minutes,
                weekday_minutes,
                enabled,
                pin,
            },
            now,
        ),
        Request::DeleteLimit { target, pin } => delete_limit(ctx, target, &pin, now),
        Request::CancelPendingLimit { target, pin } => cancel_pending_limit(ctx, target, &pin),
        Request::GrantOverride {
            target,
            seconds,
            pin,
        } => grant_override(ctx, target, seconds, &pin, now),
        Request::Categorize {
            app_id,
            primary,
            tags,
        } => categorize(ctx, app_id, primary, &tags),
        Request::ReportUsage { report } => report_usage(ctx, report),
        Request::ListManualBlocks => list_manual_blocks(ctx),
        Request::AddManualBlock { domain } => add_manual_block(ctx, &domain),
        Request::RemoveManualBlock { domain, pin } => remove_manual_block(ctx, &domain, &pin),
        _ => unimplemented!(),
    }
}

fn status_response(ctx: &Ctx) -> Response {
    let pin_configured = {
        let db = lock_db(&ctx.db);
        db.pin_hash().ok().flatten().is_some()
    };
    // Truthful tracking: reports arriving recently mean the session helper is
    // alive; otherwise only the legacy self-sampling fallback counts.
    let tracking_available = ctx.status.self_sampling
        || ctx
            .live
            .reported_within(RECENT_REPORT_SECS, ctx.clock.now_utc());

    Response::Status(StatusDto {
        agent_version: ctx.status.agent_version.clone(),
        tracker_backend: ctx.status.tracker_backend.clone(),
        enforcement_backend: ctx.status.enforcement_backend.clone(),
        filter_backend: ctx.status.filter_backend.clone(),
        tracking_available,
        blocks_encrypted_dns: ctx.status.blocks_encrypted_dns,
        strict_mode: ctx.policy.read().unwrap().strict_mode,
        pin_configured,
        show_hud_overlay: ctx.policy.read().unwrap().show_hud_overlay,
        path_level: false,
        wildcard_domains: false,
    })
}

fn day_summary(ctx: &Ctx, day: DayKey) -> Response {
    let db = lock_db(&ctx.db);
    let summary = match db.day_summary(day) {
        Ok(summary) => summary,
        Err(e) => {
            return error_internal(format!("dashboard query failed: {e}"));
        }
    };

    // Which apps are currently blocked, for honest blocked flags.
    let blocked_apps: std::collections::HashSet<i64> = db
        .blocked_subjects()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|(subject, _)| match subject {
            SubjectRef::App(id) => Some(id),
            SubjectRef::Site(_) => None,
        })
        .collect();

    // Per-app allowed seconds from limits, for progress rings.
    let limits = db.load_limits().unwrap_or_default();
    let limit_secs_for = |app_id: i64| -> Option<i64> {
        limits
            .iter()
            .find(|l| matches!(l.target, LimitTarget::App(id) if id == app_id))
            .map(|l| i64::from(l.default_minutes) * 60)
    };
    
    let snapshot = db.day_snapshot(summary.day).ok();
    let timer_expires = |target: LimitTarget| -> Option<String> {
        snapshot.as_ref().and_then(|s| s.active_timer_expires_utc(&target)).map(|t| t.to_rfc3339())
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
                timer_expires_utc: timer_expires(LimitTarget::App(a.id)),
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
                timer_expires_utc: timer_expires(LimitTarget::Category(c.id)),
            })
            .collect(),
    })
}

/// Seven-day trend plus the previous week's total. The storage layer already
/// guarantees the shape the wire promises (seven zero-filled days, oldest
/// first), so this handler is pure mapping.
fn weekly_summary(ctx: &Ctx, end_day: DayKey) -> Response {
    let db = lock_db(&ctx.db);
    let summary = match db.weekly_summary(end_day) {
        Ok(summary) => summary,
        Err(e) => return error_internal(format!("weekly summary query failed: {e}")),
    };

    Response::WeeklySummary(st_ipc::WeeklySummaryDto {
        days: summary
            .days
            .into_iter()
            .map(|d| st_ipc::DailyTotalDto {
                day: d.day,
                total_seconds: d.total_seconds,
            })
            .collect(),
        previous_week_total: summary.previous_week_total,
    })
}

fn catalog(ctx: &Ctx) -> Response {
    let db = lock_db(&ctx.db);
    let (apps, categories, limits, pending_limits) = match (
        db.list_apps(),
        db.list_categories(),
        db.list_limit_rows(),
        db.list_pending_limits(),
    ) {
        (Ok(apps), Ok(categories), Ok(limits), Ok(pending)) => (apps, categories, limits, pending),
        (Err(e), _, _, _) => return error_internal(format!("list apps: {e}")),
        (_, Err(e), _, _) => return error_internal(format!("list categories: {e}")),
        (_, _, Err(e), _) => return error_internal(format!("list limits: {e}")),
        (_, _, _, Err(e)) => return error_internal(format!("list pending limits: {e}")),
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
        pending_limits: pending_limits
            .iter()
            .filter_map(pending_limit_to_dto)
            .collect(),
    })
}

/// Currently-blocked apps, for the overlay owner. Joins `block_state` with the
/// app rows to hand back a label and key the session helper can match.
fn blocked_apps(ctx: &Ctx) -> Response {
    let db = lock_db(&ctx.db);
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
    Response::BlockedApps(st_ipc::BlockedAppsDto { blocked: out })
}

/// "Quit" from the block overlay: terminate the app's process tree. This is a
/// deliberate user action (not the removed auto-freeze), so it is PIN-gated
/// like every other mutating request.
fn close_apps(ctx: &Ctx, app_id: i64, pin: &str) -> Response {
    let db = lock_db(&ctx.db);
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
    drop(db);

    let mut processes = lock_recover(&ctx.processes, "process controller");
    let pids = match processes.find_processes(&key) {
        Ok(pids) => pids,
        Err(e) => return error_internal(format!("find processes: {e}")),
    };
    for pid in pids {
        if let Err(e) = processes.terminate(pid) {
            tracing::warn!(pid, error = %e, "terminate failed (process may have exited)");
        }
    }
    drop(processes);

    let db = lock_db(&ctx.db);
    let _ = db.clear_block(SubjectRef::App(app_id));
    accepted(ctx.clock.now_utc())
}

fn set_pin(ctx: &Ctx, new_pin: &str, current_pin: Option<&str>) -> Response {
    if new_pin.is_empty() {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "PIN cannot be empty".into(),
        };
    }
    let db = lock_db(&ctx.db);
    match db.pin_hash() {
        Ok(Some(stored)) => {
            let current = current_pin.unwrap_or("");
            // Changing an existing vault accepts either the current PIN or
            // the standing recovery code — same ownership proof either way.
            if !verify_pin(current, &stored) && !recovery_ok(&db, current) {
                return Response::Error {
                    code: ErrorCode::BadPin,
                    message: "current PIN does not match".into(),
                };
            }
        }
        Ok(None) => {}
        Err(e) => return error_internal(format!("read pin: {e}")),
    }
    rotate_vault(&db, new_pin)
}

/// Replace a forgotten PIN via its recovery code. Only the code unlocks this
/// path — by definition the user does not have the PIN — and the vault is
/// rotated so the used code stops working immediately.
fn recover_pin(ctx: &Ctx, recovery_code: &str, new_pin: &str) -> Response {
    if new_pin.is_empty() {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "PIN cannot be empty".into(),
        };
    }
    let db = lock_db(&ctx.db);
    match db.recovery_hash() {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Response::Error {
                code: ErrorCode::NotFound,
                message: "no PIN vault is configured".into(),
            }
        }
        Err(e) => return error_internal(format!("read recovery: {e}")),
    }
    if !recovery_ok(&db, recovery_code) {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "recovery code does not match".into(),
        };
    }
    rotate_vault(&db, new_pin)
}

/// Dismantle the vault. Requires the same ownership proof as changing it
/// (PIN or recovery code); idempotent when no vault exists.
fn remove_pin(ctx: &Ctx, credential: &str) -> Response {
    let db = lock_db(&ctx.db);
    let authorized = match db.pin_hash() {
        Ok(None) => true,
        Ok(Some(stored)) => verify_pin(credential, &stored) || recovery_ok(&db, credential),
        Err(e) => return error_internal(format!("read pin: {e}")),
    };
    if !authorized {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "wrong PIN or recovery code".into(),
        };
    }
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
/// the previous code, so stale codes from earlier eras are inert.
fn rotate_vault(db: &Db, new_pin: &str) -> Response {
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
    match db
        .set_pin_hash(&pin_hash)
        .and_then(|()| db.set_recovery_hash(&recovery_hash))
    {
        Ok(()) => Response::PinVault {
            recovery_code: code,
        },
        Err(e) => error_internal(format!("store vault: {e}")),
    }
}

fn set_limit(ctx: &Ctx, spec: LimitSpec, now: DateTime<Utc>) -> Response {
    let db = lock_db(&ctx.db);
    if !pin_ok(&db, &spec.pin) {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "PIN required".into(),
        };
    }
    let Some(target) = dto_to_target(&spec.target) else {
        return bad_request("unknown limit target".into());
    };
    if let Err(code) = validate_target(&db, &target) {
        return Response::Error {
            code,
            message: "target cannot carry a time limit".into(),
        };
    }

    // A failed read must never masquerade as "no existing limit": that would
    // route a loosening edit down the apply-immediately path and defeat the
    // anti-impulse cooldown precisely when the database is misbehaving.
    let existing = match db.load_limits() {
        Ok(limits) => limits.into_iter().find(|l| l.target == target),
        Err(e) => return error_internal(format!("load limits: {e}")),
    };
    let effective = match existing {
        // Tightening, a brand-new limit, and switching a limit OFF apply
        // immediately. Disabling is deliberately an *instant* escape hatch:
        // the anti-impulse cooldown exists to stop future-you from granting
        // itself more minutes, not from standing an order down entirely.
        Some(existing)
            if !spec.enabled
                || !is_loosening(&existing, spec.default_minutes, spec.weekday_minutes) =>
        {
            now
        }
        // Only loosening *minutes* while staying enabled waits out the cooldown.
        Some(_) => now + Duration::hours(ctx.policy.read().unwrap().limit_cooldown_hours.max(0)),
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
            hud: None,
            effective_utc: effective.to_rfc3339(),
        },
        Err(e) => error_internal(format!("set limit: {e}")),
    }
}

fn delete_limit(ctx: &Ctx, target: LimitTargetDto, pin: &str, now: DateTime<Utc>) -> Response {
    let db = lock_db(&ctx.db);
    if !pin_ok(&db, pin) {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "PIN required".into(),
        };
    }
    let Some(target) = dto_to_target(&target) else {
        return bad_request("unknown limit target".into());
    };
    // Deleting lifts enforcement instantly (owner decision, 2026-08): the
    // cooldown protects against loosening minutes on impulse, not against
    // standing an order down. The enforcer's next evaluation tick thaws
    // whatever the deleted order had frozen.
    match db.delete_limit_by_target(&target) {
        Ok(()) => Response::Accepted {
            hud: None,
            effective_utc: now.to_rfc3339(),
        },
        Err(e) => error_internal(format!("delete limit: {e}")),
    }
}

fn cancel_pending_limit(ctx: &Ctx, target: LimitTargetDto, pin: &str) -> Response {
    let db = lock_db(&ctx.db);
    if !pin_ok(&db, pin) {
        return Response::Error {
            code: ErrorCode::BadPin,
            message: "PIN required".into(),
        };
    }
    let Some(target) = dto_to_target(&target) else {
        return bad_request("unknown limit target".into());
    };
    match db.cancel_pending_limit(&target) {
        Ok(()) => {
            // we just use the current time from clock
            Response::Accepted {
                hud: None,
                effective_utc: ctx.clock.now_utc().to_rfc3339(),
            }
        }
        Err(e) => error_internal(format!("cancel pending limit: {e}")),
    }
}

fn grant_override(
    ctx: &Ctx,
    target: LimitTargetDto,
    seconds: i64,
    pin: &str,
    now: DateTime<Utc>,
) -> Response {
    let db = lock_db(&ctx.db);
    if ctx.policy.read().unwrap().strict_mode {
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
        return bad_request("unknown limit target".into());
    };
    let seconds = seconds.clamp(1, 24 * 3600);
    // Attribute the bonus to the user's local day — the same computation the
    // sampler/enforcer use — not to the UTC calendar day. Hard-coding offset
    // zero put evening overrides west of UTC onto tomorrow's budget.
    let day = DayKey::from_utc(
        now,
        ctx.clock.local_offset_seconds(),
        ctx.policy.read().unwrap().day_start_minutes,
    );
    match db.grant_override(&target, day, seconds, now, Some("user override")) {
        Ok(()) => accepted(now),
        Err(e) => error_internal(format!("grant override: {e}")),
    }
}

fn categorize(ctx: &Ctx, app_id: i64, primary: i64, tags: &[i64]) -> Response {
    let mut db = lock_db(&ctx.db);
    match db.set_app_categories(app_id, primary, tags, true) {
        Ok(()) => accepted(ctx.clock.now_utc()),
        Err(e) => error_internal(format!("categorise: {e}")),
    }
}

/// Ingest one session-helper report.
///
/// Pipeline, in order:
///
/// 1. **Parse & sanity-check** each `observed_at_utc` against the agent clock.
///    Unparsable stamps are dropped; timestamps wildly in the future (beyond
///    [`REPORT_MAX_FUTURE_SECS`]) or stale (beyond [`REPORT_MAX_AGE_SECS`])
///    are dropped too — both would corrupt day buckets. Moderate past skew is
///    credited but logged, so chronic clock drift is visible without throwing
///    away real usage.
/// 2. **Dedupe**: identical `(app_key, observed_at)` pairs collapse, within
///    the batch and against a TTL'd ring of recently ingested pairs (the wire
///    contract requires idempotency because a lost reply forces a resend).
/// 3. **Chain**: observations are sorted ascending, then consecutive active
///    observations of the same app bridge into runs whenever their gap is at
///    most [`CHAIN_GAP_SECS`]. Each bridging step credits `[previous_end,
///    this_end]`, so credit accumulates incrementally across batches and no
///    per-batch bookkeeping is needed. An observation whose `idle_seconds`
///    reaches the configured threshold closes the app's open run instead —
///    focused-but-idle time accrues nothing, mirroring local-sampler idle
///    semantics. The first observation of a run credits nothing by itself:
///    duration needs two endpoints, which is also why helpers flush repeatedly
///    while an app stays focused rather than sending one final sample.
/// 4. **Bucket & persist**: each credited span is split at local-day
///    boundaries (midnight rollover mid-batch lands on the right days) and
///    written through the same classify-and-record path local sampling uses.
///
/// The reply is always [`Response::Accepted`] carrying the agent's ingest
/// instant unless something failed *before* anything was persisted (unknown
/// taxonomy, poisoned storage); the helper compares `effective_utc` with its
/// own send clock to measure skew.
fn report_usage(ctx: &Ctx, report: ReportUsageDto) -> Response {
    let now = ctx.clock.now_utc();
    let tz_offset = ctx.clock.local_offset_seconds();
    let day_start = ctx.policy.read().unwrap().day_start_minutes;
    let idle_threshold = ctx.policy.read().unwrap().idle_threshold_secs.max(1);

    // 1. Parse and sanity-check timestamps.
    let mut parsed: Vec<(DateTime<Utc>, ObservationDto)> =
        Vec::with_capacity(report.observations.len());
    for obs in report.observations {
        let Ok(t) = DateTime::parse_from_rfc3339(&obs.observed_at_utc) else {
            tracing::warn!(
                app = %obs.app_key,
                observed_at = %obs.observed_at_utc,
                "dropping observation with unparsable timestamp"
            );
            continue;
        };
        let t = t.with_timezone(&Utc);
        if t > now + Duration::seconds(REPORT_MAX_FUTURE_SECS) {
            tracing::warn!(
                app = %obs.app_key,
                observed_at = %t,
                now = %now,
                "dropping wildly-future observation"
            );
            continue;
        }
        if t < now - Duration::seconds(REPORT_MAX_AGE_SECS) {
            tracing::warn!(app = %obs.app_key, observed_at = %t, "dropping stale observation");
            continue;
        }
        if t < now - Duration::seconds(REPORT_SKEW_WARN_SECS) {
            tracing::warn!(
                app = %obs.app_key,
                behind_secs = (now - t).num_seconds(),
                "observation lags agent clock; crediting anyway"
            );
        }
        parsed.push((t, obs));
    }

    // 2. Dedupe within the batch and against recent batches.
    {
        let mut seen = lock_recover(&ctx.live.seen_observations, "observation dedupe ring");
        let cutoff = (now - Duration::seconds(DEDUPE_TTL_SECS)).timestamp();
        seen.retain(|(_, ts)| *ts >= cutoff);
        parsed.retain(|(t, obs)| {
            let entry = (obs.app_key.to_db_string(), t.timestamp());
            if seen.contains(&entry) {
                false
            } else {
                seen.push(entry);
                true
            }
        });
        if seen.len() > DEDUPE_CAP {
            let excess = seen.len() - DEDUPE_CAP;
            seen.drain(..excess);
        }
    }

    // 3. Order and chain into credited spans.
    parsed.sort_by_key(|(t, _)| *t);
    let mut credited: Vec<(AppKey, DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    {
        let mut chains = lock_recover(&ctx.live.open_chains, "open usage chains");
        for (t, obs) in parsed.clone() {
            let key_string = obs.app_key.to_db_string();
            ctx.live.note_focus(obs.app_key.clone(), t);
            if i64::from(obs.idle_seconds) >= idle_threshold {
                chains.remove(&key_string);
                continue;
            }
            match chains.get_mut(&key_string) {
                Some(state) if (t - state.last).num_seconds() <= CHAIN_GAP_SECS => {
                    credited.push((obs.app_key.clone(), state.last, t));
                    state.last = t;
                }
                _ => {
                    chains.insert(key_string, ChainState { last: t });
                }
            }
        }
    }

    // 4. Split across day boundaries and persist through the shared path.
    if !credited.is_empty() {
        let mut db = lock_db(&ctx.db);
        let default_category = match db.category_id(st_core::category::UNCATEGORIZED_SLUG) {
            Ok(id) => id,
            Err(e) => return error_internal(format!("uncategorized category missing: {e}")),
        };
        for (key, start, end) in credited {
            let span = PendingInterval {
                key: key.clone(),
                display_name: key.basename().to_string(),
                start,
                end,
                day: DayKey(0),
            };
            for piece in split_by_day(&span, tz_offset, day_start) {
                if let Err(e) = persist(&mut db, &piece, default_category, now) {
                    tracing::error!(
                        error = %e,
                        app = %piece.key,
                        "failed to persist reported interval"
                    );
                }
            }
        }
    }

    ctx.live.note_report(now);
    
    // 6. Compute HUD overlay state for the currently focused app.
    let mut hud = None;
    if ctx.policy.read().unwrap().show_hud_overlay {
        if let Some((_, obs)) = parsed.last() {
            let db_guard = lock_db(&ctx.db);
            let today = DayKey::from_utc(now, tz_offset, day_start);
            if let Ok(snap) = db_guard.day_snapshot(today) {
                if let Ok(Some(app_id)) = db_guard.app_id_for_key(&obs.app_key) {
                    if let Ok(Some(record)) = db_guard.app_record(app_id) {
                        let limits = db_guard.load_limits().unwrap_or_default();
                        let engine = st_core::limits::LimitEngine::with_default_warnings(limits);
                        let decision = engine.evaluate(
                            record.id,
                            &record.all_categories(),
                            true, // we assume it's blockable for the HUD check
                            today.weekday_index().unwrap_or(0),
                            &snap,
                            now,
                        );
                        let match_res = match decision {
                            st_core::limits::Decision::Allow { remaining_secs, binding: Some(target) } => Some((remaining_secs, target)),
                            st_core::limits::Decision::Warn { remaining_secs, binding: target, .. } => Some((remaining_secs, target)),
                            _ => None,
                        };
                        if let Some((remaining_secs, target)) = match_res {
                            let is_timer = snap.active_timer_expires_utc(&target).is_some();
                            hud = Some(st_ipc::HudStateDto {
                                remaining_secs,
                                is_timer,
                            });
                        }
                    }
                }
            }
        }
    }

    Response::Accepted {
        hud,
        effective_utc: now.to_rfc3339(),
    }

}

/// Split a usage span into per-local-day pieces so midnight rollover mid-span
/// charges each calendar day for its own share (same rule the sampler applies
/// to all-night sessions).
fn split_by_day(
    interval: &PendingInterval,
    tz_offset_secs: i32,
    day_start_minutes: i64,
) -> Vec<PendingInterval> {
    if interval.end <= interval.start {
        return Vec::new();
    }
    let start_day = DayKey::from_utc(interval.start, tz_offset_secs, day_start_minutes);
    let end_day = DayKey::from_utc(interval.end, tz_offset_secs, day_start_minutes);
    if start_day == end_day {
        return vec![PendingInterval {
            day: start_day,
            ..interval.clone()
        }];
    }
    match start_day.end_utc(tz_offset_secs, day_start_minutes) {
        Some(boundary) if boundary > interval.start && boundary < interval.end => {
            let mut head = interval.clone();
            head.day = start_day;
            head.end = boundary;
            let mut tail = interval.clone();
            tail.start = boundary;
            let mut out = vec![head];
            out.extend(split_by_day(&tail, tz_offset_secs, day_start_minutes));
            out
        }
        // An unresolvable boundary cannot be split honestly; charge the whole
        // span to the day it started in rather than dropping real usage.
        _ => vec![PendingInterval {
            day: start_day,
            ..interval.clone()
        }],
    }
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

/// A limit is "loosening" if any time dimension went up. Disabling is NOT
/// counted here: it is handled one branch above as an instant action, by
/// design (the cooldown guards minute-loosening, not standing a order down).
fn is_loosening(
    existing: &st_core::limits::Limit,
    default_minutes: u32,
    weekday_minutes: [Option<u32>; 7],
) -> bool {
    if default_minutes > existing.default_minutes {
        return true;
    }
    weekday_minutes
        .iter()
        .zip(existing.weekday_minutes.iter())
        .any(|(new, old)| {
            let new_effective = new.unwrap_or(default_minutes);
            let old_effective = old.unwrap_or(existing.default_minutes);
            new_effective > old_effective
        })
}

fn validate_target(db: &Db, target: &LimitTarget) -> Result<(), ErrorCode> {
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

/// Whether `code` matches the standing recovery code. Input is normalised so
/// lowercase / missing dashes / stray spaces still verify against the paper
/// form.
fn recovery_ok(db: &Db, code: &str) -> bool {
    match db.recovery_hash() {
        Ok(Some(stored)) => verify_pin(&normalize_recovery_code(code), &stored),
        _ => false,
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
        timer_expires_utc: None,
    })
}

fn pending_limit_to_dto(row: &st_storage::PendingLimitRow) -> Option<st_ipc::PendingLimitDto> {
    let target = match row.target {
        Some(LimitTarget::App(id)) => LimitTargetDto::App { id },
        Some(LimitTarget::Category(id)) => LimitTargetDto::Category { id },
        Some(LimitTarget::Total) => LimitTargetDto::Total,
        None => return None,
    };
    Some(st_ipc::PendingLimitDto {
        id: row.id,
        target,
        action: row.action.clone(),
        default_minutes: row.default_minutes.map(|v| v as u32),
        weekday_minutes: row.weekday_minutes,
        enabled: row.enabled,
        effective_from_utc: row.effective_from_utc.clone(),
    })
}

fn accepted(effective_utc: DateTime<Utc>) -> Response {
    Response::Accepted {
        hud: None,
        effective_utc: effective_utc.to_rfc3339(),
    }
}

fn bad_request(message: String) -> Response {
    Response::Error {
        code: ErrorCode::BadRequest,
        message,
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
    use st_core::clock::TestClock;
    use st_core::limits::Limit;
    use st_core::limits::UsageSnapshot;
    use st_ipc::IpcError;

    fn base() -> Limit {
        Limit::new(1, LimitTarget::Total, 30)
    }

    #[test]
    fn tightening_is_not_loosening() {
        let existing = base();
        assert!(!is_loosening(&existing, 15, [None; 7]));
        assert!(!is_loosening(&existing, 30, [None; 7]));
    }

    #[test]
    fn increasing_minutes_is_loosening() {
        let existing = base();
        assert!(is_loosening(&existing, 45, [None; 7]));
    }

    #[test]
    fn raising_a_weekday_override_is_loosening() {
        let existing = base();
        let mut new = [None; 7];
        new[6] = Some(120);
        assert!(is_loosening(&existing, 30, new));
    }

    #[test]
    fn lowering_a_weekday_override_is_tightening() {
        let mut existing = base();
        existing.weekday_minutes[6] = Some(120);
        let mut new = [None; 7];
        new[6] = Some(60);
        assert!(!is_loosening(&existing, 30, new));
    }

    #[test]
    fn mixed_changes_are_treated_as_loosening() {
        let existing = base();
        let mut new = [None; 7];
        new[0] = Some(15); // tighter on Monday
        new[6] = Some(120); // looser on Sunday
        assert!(is_loosening(&existing, 30, new));
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

    // -- Handler-level fixtures. ---------------------------------------------

    /// Records terminations so the overlay's Quit action can be asserted.
    struct FakeProcesses {
        pids: Vec<u32>,
        terminated: Arc<Mutex<Vec<u32>>>,
    }

    impl Default for FakeProcesses {
        fn default() -> Self {
            Self {
                pids: vec![111, 222],
                terminated: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl ProcessController for FakeProcesses {
        fn find_processes(&mut self, _key: &AppKey) -> st_core::platform::PlatformResult<Vec<u32>> {
            Ok(self.pids.clone())
        }

        fn freeze(&mut self, _pid: u32) -> st_core::platform::PlatformResult<()> {
            Ok(())
        }

        fn thaw(&mut self, _pid: u32) -> st_core::platform::PlatformResult<()> {
            Ok(())
        }

        fn terminate(&mut self, pid: u32) -> st_core::platform::PlatformResult<()> {
            self.terminated.lock().expect("log").push(pid);
            Ok(())
        }

        fn backend(&self) -> &'static str {
            "fake"
        }
    }

    fn test_ctx(db: Db, clock: TestClock) -> Ctx {
        test_ctx_with_policy(db, clock, |_| ())
    }

    /// A second context over an existing context's database, for tests that
    /// need two moments in time against the same storage.
    fn test_ctx_with_db_handle(db: &Arc<Mutex<Db>>, clock: TestClock) -> Ctx {
        Ctx {
            db: Arc::clone(db),
            status: Arc::new(StatusInfo {
                agent_version: "test".into(),
                tracker_backend: "fake".into(),
                enforcement_backend: "fake".into(),
                filter_backend: "none".into(),
                self_sampling: false,
                blocks_encrypted_dns: false,
            }),
            policy: Arc::new(Policy {
                limit_cooldown_hours: 24,
                strict_mode: false,
                day_start_minutes: 0,
                idle_threshold_secs: 60,
                show_hud_overlay: true,
            }),
            processes: Arc::new(Mutex::new(Box::new(FakeProcesses::default()))),
            clock: Arc::new(clock),
            live: Arc::new(Live::default()),
        }
    }

    fn test_ctx_with_policy(db: Db, clock: TestClock, tweak: impl FnOnce(&mut Policy)) -> Ctx {
        let mut policy = Policy {
            limit_cooldown_hours: 24,
            strict_mode: false,
            day_start_minutes: 0,
            idle_threshold_secs: 60,
            show_hud_overlay: true,
        };
        tweak(&mut policy);
        Ctx {
            db: Arc::new(Mutex::new(db)),
            status: Arc::new(StatusInfo {
                agent_version: "test".into(),
                tracker_backend: "fake".into(),
                enforcement_backend: "fake".into(),
                filter_backend: "none".into(),
                self_sampling: false,
                blocks_encrypted_dns: false,
            }),
            policy: Arc::new(std::sync::RwLock::new(policy)),
            processes: Arc::new(Mutex::new(Box::new(FakeProcesses::default()))),
            clock: Arc::new(clock),
            live: Arc::new(Live::default()),
        }
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .expect("valid rfc3339")
            .with_timezone(&Utc)
    }

    fn error_code(response: &Response) -> Option<ErrorCode> {
        match response {
            Response::Error { code, .. } => Some(*code),
            _ => None,
        }
    }

    fn expect_accepted(response: Response, expected: DateTime<Utc>) {
        match response {
            Response::Accepted {
                hud: None,
                effective_utc,
            } => assert_eq!(
                DateTime::parse_from_rfc3339(&effective_utc)
                    .expect("rfc3339")
                    .with_timezone(&Utc),
                expected,
                "Accepted must carry the ingest/effective instant"
            ),
            other => panic!("expected Accepted at {expected}, got {other:?}"),
        }
    }

    fn seed_game_app(db: &mut Db, path: &str) -> i64 {
        let uncat = db.category_id("uncategorized").expect("uncat");
        let games = db.category_id("games").expect("games");
        let key = AppKey::windows_exe(path);
        let id = db
            .upsert_app(&key, "Steam", None, uncat, at("2026-08-20T09:00:00Z"))
            .expect("app");
        db.set_app_categories(id, games, &[], false)
            .expect("classify");
        id
    }

    fn record_usage(db: &mut Db, app: i64, secs: i64, day: DayKey) {
        db.record_interval(&st_core::model::UsageInterval {
            subject: SubjectRef::App(app),
            session_id: "test".into(),
            start: at("2026-08-20T10:00:00Z"),
            end: at("2026-08-20T10:00:00Z") + Duration::seconds(secs),
            day_key: day,
        })
        .expect("interval");
    }

    fn obs(app_key: &str, observed_at: &str, idle_seconds: u32) -> ObservationDto {
        ObservationDto {
            app_key: AppKey::windows_exe(app_key),
            window_title: None,
            idle_seconds,
            observed_at_utc: observed_at.into(),
        }
    }

    fn report_of(observations: Vec<ObservationDto>) -> Request {
        Request::ReportUsage {
            report: ReportUsageDto { observations },
        }
    }

    fn used_secs_for_key(db: &mut Db, key: &str, day: DayKey) -> i64 {
        let app = db
            .app_id_for_key(&AppKey::windows_exe(key))
            .expect("lookup")
            .expect("ingested app exists");
        db.day_snapshot(day)
            .expect("snapshot")
            .seconds_used(&LimitTarget::App(app))
    }

    // -- Handler behaviour. ---------------------------------------------------

    /// Pull the one-time code out of a PinVault reply, failing loudly on any
    /// other variant.
    fn expect_vault(response: Response) -> String {
        match response {
            Response::PinVault { recovery_code } => recovery_code,
            other => panic!("expected PinVault, got {other:?}"),
        }
    }

    #[test]
    fn set_pin_first_set_then_change_requires_the_current_pin() {
        let ctx = test_ctx(
            Db::open_in_memory().expect("db"),
            TestClock::new(at("2026-08-20T12:00:00Z"), 0),
        );

        let first_code = expect_vault(handle(
            &ctx,
            Request::SetPin {
                new_pin: "1234".into(),
                current_pin: None,
            },
        ));

        // Changing without (or with a wrong) current PIN is refused...
        for wrong in [None, Some("0000")] {
            assert_eq!(
                error_code(&handle(
                    &ctx,
                    Request::SetPin {
                        new_pin: "5678".into(),
                        current_pin: wrong.map(Into::into)
                    }
                )),
                Some(ErrorCode::BadPin)
            );
        }

        // ...and with the right current PIN the change lands.
        let second_code = expect_vault(handle(
            &ctx,
            Request::SetPin {
                new_pin: "5678".into(),
                current_pin: Some("1234".into()),
            },
        ));
        let stored = lock_db(&ctx.db)
            .pin_hash()
            .expect("read")
            .expect("configured");
        assert!(verify_pin("5678", &stored), "new PIN must be live");
        assert_ne!(
            first_code, second_code,
            "every vault rotation must retire the old recovery code"
        );
    }

    #[test]
    fn a_recovery_code_resets_a_forgotten_pin_and_rotates_itself() {
        let ctx = test_ctx(
            Db::open_in_memory().expect("db"),
            TestClock::new(at("2026-08-20T12:00:00Z"), 0),
        );
        let code = expect_vault(handle(
            &ctx,
            Request::SetPin {
                new_pin: "1234".into(),
                current_pin: None,
            },
        ));

        // Wrong code refused; right code — even sloppily typed — accepted.
        assert_eq!(
            error_code(&handle(
                &ctx,
                Request::RecoverPin {
                    recovery_code: "AAAA-BBBB-CCCC-DDDD".into(),
                    new_pin: "9999".into()
                }
            )),
            Some(ErrorCode::BadPin)
        );
        expect_vault(handle(
            &ctx,
            Request::RecoverPin {
                // Lowercase, no dashes, stray spaces: must still verify.
                recovery_code: format!(" {} ", code.replace('-', "").to_lowercase()),
                new_pin: "9999".into(),
            },
        ));

        let db_guard = lock_db(&ctx.db);
        let stored = db_guard.pin_hash().expect("read").expect("configured");
        assert!(verify_pin("9999", &stored), "recovered PIN must be live");

        // Rotation retired the used code...
        drop(db_guard);
        assert_eq!(
            error_code(&handle(
                &ctx,
                Request::RecoverPin {
                    recovery_code: code.clone(),
                    new_pin: "1111".into()
                }
            )),
            Some(ErrorCode::BadPin)
        );

        // ...but a later change can present either the current PIN or the
        // standing recovery code; each rotation mints a fresh code.
        let fresh = expect_vault(handle(
            &ctx,
            Request::SetPin {
                new_pin: "2222".into(),
                current_pin: Some("9999".into()),
            },
        ));
        assert_ne!(fresh, code);
    }

    #[test]
    fn removing_the_pin_requires_a_credential_and_clears_the_whole_vault() {
        let ctx = test_ctx(
            Db::open_in_memory().expect("db"),
            TestClock::new(at("2026-08-20T12:00:00Z"), 0),
        );
        let code = expect_vault(handle(
            &ctx,
            Request::SetPin {
                new_pin: "1234".into(),
                current_pin: None,
            },
        ));

        assert_eq!(
            error_code(&handle(
                &ctx,
                Request::RemovePin {
                    credential: "0000".into()
                }
            )),
            Some(ErrorCode::BadPin),
            "no credential, no removal"
        );

        // The recovery code is as good as the PIN for standing down the gate.
        expect_accepted(
            handle(&ctx, Request::RemovePin { credential: code }),
            at("2026-08-20T12:00:00Z"),
        );
        // Scoped so the guard cannot deadlock the idempotent call below.
        {
            let db_guard = lock_db(&ctx.db);
            assert_eq!(db_guard.pin_hash().expect("read"), None);
            assert_eq!(db_guard.recovery_hash().expect("read"), None);
        }

        // Idempotent when no vault exists.
        expect_accepted(
            handle(
                &ctx,
                Request::RemovePin {
                    credential: String::new(),
                },
            ),
            at("2026-08-20T12:00:00Z"),
        );
    }

    #[test]
    fn set_limit_tightening_applies_immediately() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
        db.upsert_limit(
            &Limit::new(1, LimitTarget::Category(games), 30),
            at("2026-08-20T09:00:00Z"),
        )
        .expect("seed limit");

        let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
        let response = handle(
            &ctx,
            Request::SetLimit {
                target: LimitTargetDto::Category { id: games },
                default_minutes: 15,
                weekday_minutes: [None; 7],
                enabled: true,
                pin: String::new(),
            },
        );

        expect_accepted(response, at("2026-08-20T12:00:00Z"));
        let limits = lock_db(&ctx.db).load_limits().expect("limits");
        assert_eq!(limits[0].default_minutes, 15, "tightened value is live");
    }

    #[test]
    fn set_limit_loosening_waits_out_the_cooldown_then_promotes() {
        let db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        db.upsert_limit(
            &Limit::new(1, LimitTarget::Category(games), 30),
            at("2026-08-20T09:00:00Z"),
        )
        .expect("seed limit");

        let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
        let response = handle(
            &ctx,
            Request::SetLimit {
                target: LimitTargetDto::Category { id: games },
                default_minutes: 120,
                weekday_minutes: [None; 7],
                enabled: true,
                pin: String::new(),
            },
        );

        expect_accepted(response, at("2026-08-21T12:00:00Z"));
        let mut db_guard = lock_db(&ctx.db);
        assert_eq!(
            db_guard.load_limits().expect("limits")[0].default_minutes,
            30,
            "the tighter value must keep being enforced during cooldown"
        );
        db_guard
            .promote_pending_limits(at("2026-08-21T12:00:00Z"))
            .expect("promote");
        assert_eq!(
            db_guard.load_limits().expect("limits")[0].default_minutes,
            120,
            "promotion applies the loosened value"
        );
    }

    #[test]
    fn a_failed_limit_read_is_internal_rather_than_fail_open() {
        let db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        db.upsert_limit(
            &Limit::new(1, LimitTarget::Category(games), 30),
            at("2026-08-20T09:00:00Z"),
        )
        .expect("seed limit");
        // Corrupt the column load_limits parses, forcing the read to fail.
        db.conn()
            .execute("UPDATE limits SET weekday_minutes = 'not-json'", [])
            .expect("corrupt");

        let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
        let response = handle(
            &ctx,
            Request::SetLimit {
                target: LimitTargetDto::Category { id: games },
                default_minutes: 15,
                weekday_minutes: [None; 7],
                enabled: true,
                pin: String::new(),
            },
        );
        assert_eq!(error_code(&response), Some(ErrorCode::Internal));
    }

    #[test]
    fn a_non_limitable_category_rejects_the_limit() {
        let db = Db::open_in_memory().expect("db");
        let dev = db.category_id("development").expect("development");
        let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));

        let response = handle(
            &ctx,
            Request::SetLimit {
                target: LimitTargetDto::Category { id: dev },
                default_minutes: 30,
                weekday_minutes: [None; 7],
                enabled: true,
                pin: String::new(),
            },
        );
        assert_eq!(error_code(&response), Some(ErrorCode::NotLimitable));
    }

    #[test]
    fn delete_limit_applies_instantly() {
        let db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        db.upsert_limit(
            &Limit::new(1, LimitTarget::Category(games), 30),
            at("2026-08-20T09:00:00Z"),
        )
        .expect("seed limit");

        let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
        let response = handle(
            &ctx,
            Request::DeleteLimit {
                target: LimitTargetDto::Category { id: games },
                pin: String::new(),
            },
        );
        // Instant by design (owner decision, 2026-08): standing an order down
        // is not an impulse the cooldown needs to guard.
        expect_accepted(response, at("2026-08-20T12:00:00Z"));

        let db_guard = lock_db(&ctx.db);
        assert!(db_guard.load_limits().expect("limits").is_empty());
    }

    #[test]
    fn disabling_a_limit_applies_instantly_while_loosening_still_waits() {
        let db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        db.upsert_limit(
            &Limit::new(1, LimitTarget::Category(games), 30),
            at("2026-08-20T09:00:00Z"),
        )
        .expect("seed limit");

        // Disable: instant.
        let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
        let response = handle(
            &ctx,
            Request::SetLimit {
                target: LimitTargetDto::Category { id: games },
                default_minutes: 30,
                weekday_minutes: [None; 7],
                enabled: false,
                pin: String::new(),
            },
        );
        expect_accepted(response, at("2026-08-20T12:00:00Z"));
        assert!(
            !lock_db(&ctx.db).load_limits().expect("limits")[0].enabled,
            "the disabled state must be live immediately"
        );

        // Loosening minutes while enabled still queues behind the cooldown.
        let ctx = test_ctx_with_db_handle(&ctx.db, TestClock::new(at("2026-08-20T12:05:00Z"), 0));
        let response = handle(
            &ctx,
            Request::SetLimit {
                target: LimitTargetDto::Category { id: games },
                default_minutes: 120,
                weekday_minutes: [None; 7],
                enabled: true,
                pin: String::new(),
            },
        );
        expect_accepted(response, at("2026-08-21T12:05:00Z"));
        let db_guard = lock_db(&ctx.db);
        assert_eq!(
            db_guard.load_limits().expect("limits")[0].default_minutes,
            30,
            "the tighter value keeps being enforced during cooldown"
        );
    }

    /// Regression for bug 2: an override granted at 19:00 local in UTC-7 must
    /// land on *today's* budget, not on tomorrow's UTC calendar day.
    #[test]
    fn grant_override_lands_on_the_local_day_not_the_utc_day() {
        // 02:00 UTC on the 21st == 19:00 local on the 20th.
        let utc_minus_seven = -7 * 3600;
        let ctx = test_ctx(
            Db::open_in_memory().expect("db"),
            TestClock::new(at("2026-08-21T02:00:00Z"), utc_minus_seven),
        );

        let response = handle(
            &ctx,
            Request::GrantOverride {
                target: LimitTargetDto::Total,
                seconds: 900,
                pin: String::new(),
            },
        );
        expect_accepted(response, at("2026-08-21T02:00:00Z"));

        let db_guard = lock_db(&ctx.db);
        let today = db_guard.day_snapshot(DayKey(20260820)).expect("snap");
        let expires = today
            .active_timer_expires_utc(&LimitTarget::Total)
            .expect("timer expires");
        assert_eq!(
            expires
                .signed_duration_since(at("2026-08-21T02:00:00Z"))
                .num_seconds(),
            900,
            "override credited to the local day"
        );
        let utc_day = db_guard.day_snapshot(DayKey(20260821)).expect("snap");
        assert!(
            utc_day
                .active_timer_expires_utc(&LimitTarget::Total)
                .is_none(),
            "the UTC calendar day must not receive the bonus"
        );
    }

    #[test]
    fn grant_override_refused_outright_in_strict_mode() {
        let ctx = test_ctx_with_policy(
            Db::open_in_memory().expect("db"),
            TestClock::new(at("2026-08-20T12:00:00Z"), 0),
            |p| p.strict_mode = true,
        );
        let response = handle(
            &ctx,
            Request::GrantOverride {
                target: LimitTargetDto::Total,
                seconds: 900,
                pin: String::new(),
            },
        );
        assert_eq!(error_code(&response), Some(ErrorCode::StrictMode));
    }

    #[test]
    fn close_apps_terminates_the_process_tree_and_clears_the_block() {
        let mut db = Db::open_in_memory().expect("db");
        let app = seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
        db.set_block(
            SubjectRef::App(app),
            "limit",
            at("2026-08-20T12:00:00Z"),
            None,
        )
        .expect("block");

        // A shared log the fake writes into, so the test can assert what the
        // controller (hidden behind `Box<dyn ProcessController>`) did.
        let terminated_log = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
        ctx.processes = Arc::new(Mutex::new(Box::new(FakeProcesses {
            pids: vec![111, 222],
            terminated: Arc::clone(&terminated_log),
        })));

        let response = handle(
            &ctx,
            Request::CloseApps {
                app_id: app,
                pin: String::new(),
            },
        );
        expect_accepted(response, at("2026-08-20T12:00:00Z"));
        assert!(!lock_db(&ctx.db)
            .is_blocked(SubjectRef::App(app))
            .expect("check"));

        assert_eq!(
            terminated_log.lock().expect("log").clone(),
            vec![111, 222],
            "every pid in the process tree must be terminated"
        );
    }

    #[test]
    fn close_apps_requires_a_valid_pin_once_configured() {
        let mut db = Db::open_in_memory().expect("db");
        let app = seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
        db.set_pin_hash(&hash_pin("9999").expect("hash"))
            .expect("set pin");

        let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
        let response = handle(
            &ctx,
            Request::CloseApps {
                app_id: app,
                pin: "1111".into(),
            },
        );
        assert_eq!(error_code(&response), Some(ErrorCode::BadPin));
    }

    #[test]
    fn blocked_apps_reports_label_and_key_for_the_overlay_owner() {
        let mut db = Db::open_in_memory().expect("db");
        let app = seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
        db.set_block(
            SubjectRef::App(app),
            "limit",
            at("2026-08-20T12:00:00Z"),
            None,
        )
        .expect("block");

        let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
        let response = handle(&ctx, Request::BlockedApps);
        let Response::BlockedApps(dto) = response else {
            panic!("expected BlockedApps, got {response:?}");
        };
        assert_eq!(dto.blocked.len(), 1);
        assert_eq!(dto.blocked[0].app_id, app);
        assert_eq!(dto.blocked[0].label, "Steam");
        assert_eq!(
            dto.blocked[0].app_key,
            "win-exe:c:\\games\\steam\\steam.exe"
        );
    }

    #[test]
    fn catalog_maps_apps_categories_and_limits() {
        let mut db = Db::open_in_memory().expect("db");
        let games = db.category_id("games").expect("games");
        let app = seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
        db.upsert_limit(
            &Limit::new(7, LimitTarget::App(app), 45),
            at("2026-08-20T09:00:00Z"),
        )
        .expect("limit");

        let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
        let Response::Catalog(dto) = handle(&ctx, Request::Catalog) else {
            panic!("expected Catalog");
        };

        assert_eq!(dto.apps.len(), 1);
        assert_eq!(dto.apps[0].id, app);
        assert_eq!(dto.apps[0].key, "win-exe:c:\\games\\steam\\steam.exe");
        assert_eq!(dto.apps[0].primary_category, games);

        assert!(dto
            .categories
            .iter()
            .any(|c| c.slug == "games" && c.kind == "limitable"));
        assert!(dto
            .categories
            .iter()
            .any(|c| c.slug == "development" && c.kind == "never_block"));

        assert_eq!(dto.limits.len(), 1);
        assert_eq!(
            dto.limits[0].target,
            LimitTargetDto::App { id: app },
            "the limit must point at the seeded app"
        );
        assert_eq!(dto.limits[0].default_minutes, 45);
    }

    #[test]
    fn day_summary_maps_rows_limit_seconds_and_blocked_flags() {
        let mut db = Db::open_in_memory().expect("db");
        let app = seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
        record_usage(&mut db, app, 600, DayKey(20260820));
        db.upsert_limit(
            &Limit::new(3, LimitTarget::App(app), 30),
            at("2026-08-20T09:00:00Z"),
        )
        .expect("limit");
        db.set_block(
            SubjectRef::App(app),
            "limit",
            at("2026-08-20T12:00:00Z"),
            None,
        )
        .expect("block");

        let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
        let Response::DaySummary(dto) = handle(
            &ctx,
            Request::DaySummary {
                day: DayKey(20260820),
            },
        ) else {
            panic!("expected DaySummary");
        };

        assert_eq!(dto.total_seconds, 600);
        assert_eq!(dto.apps.len(), 1);
        assert_eq!(dto.apps[0].id, app);
        assert_eq!(dto.apps[0].seconds, 600);
        assert_eq!(dto.apps[0].limit_seconds, Some(1800));
        assert!(dto.apps[0].blocked, "the block must be surfaced honestly");
        assert_eq!(dto.categories.len(), 1);
        assert_eq!(dto.categories[0].seconds, 600);
    }

    #[test]
    fn weekly_summary_maps_seven_zero_filled_days_oldest_first_with_previous_week_total() {
        let mut db = Db::open_in_memory().expect("db");
        let app = seed_game_app(&mut db, "C:\\games\\steam\\steam.exe");
        // One day inside the reported week (2026-08-28..09-03, crossing the
        // month boundary), one day in the previous week (..08-27).
        record_usage(&mut db, app, 600, DayKey(20260829));
        record_usage(&mut db, app, 1200, DayKey(20260827));

        let ctx = test_ctx(db, TestClock::new(at("2026-08-20T12:00:00Z"), 0));
        let Response::WeeklySummary(dto) = handle(
            &ctx,
            Request::WeeklySummary {
                end_day: DayKey(20260903),
            },
        ) else {
            panic!("expected WeeklySummary");
        };

        assert_eq!(dto.days.len(), 7, "always exactly seven days");
        assert_eq!(dto.days[0].day, DayKey(20260828), "oldest first");
        assert_eq!(
            dto.days[0].total_seconds, 0,
            "empty day is zero, not missing"
        );
        assert_eq!(dto.days[1].day, DayKey(20260829));
        assert_eq!(dto.days[1].total_seconds, 600);
        assert_eq!(dto.days[6].day, DayKey(20260903), "end day is last");
        assert_eq!(dto.days[6].total_seconds, 0);
        assert_eq!(dto.previous_week_total, 1200);
    }

    #[test]
    fn weekly_summary_with_an_invalid_day_key_is_an_internal_error() {
        let ctx = test_ctx(
            Db::open_in_memory().expect("db"),
            TestClock::new(at("2026-08-20T12:00:00Z"), 0),
        );
        let response = handle(
            &ctx,
            Request::WeeklySummary {
                end_day: DayKey(20260230), // February 30th does not exist.
            },
        );
        assert_eq!(error_code(&response), Some(ErrorCode::Internal));
    }

    // -- Report ingestion. ----------------------------------------------------

    #[test]
    fn report_usage_credits_ordered_active_observations_regardless_of_wire_order() {
        let ctx = test_ctx(
            Db::open_in_memory().expect("db"),
            TestClock::new(at("2026-08-25T10:05:00Z"), 0),
        );

        // Deliberately shuffled: t+60 first, then t and t+30.
        let response = handle(
            &ctx,
            report_of(vec![
                obs("c:\\apps\\game.exe", "2026-08-25T10:01:00Z", 0),
                obs("c:\\apps\\game.exe", "2026-08-25T10:00:00Z", 0),
                obs("c:\\apps\\game.exe", "2026-08-25T10:00:30Z", 0),
            ]),
        );
        expect_accepted(response, at("2026-08-25T10:05:00Z"));

        let mut db_guard = lock_db(&ctx.db);
        assert_eq!(
            used_secs_for_key(&mut db_guard, "c:\\apps\\game.exe", DayKey(20260825)),
            60
        );
    }

    #[test]
    fn report_usage_idle_observation_breaks_accrual_until_activity_resumes() {
        let ctx = test_ctx(
            Db::open_in_memory().expect("db"),
            TestClock::new(at("2026-08-25T10:05:00Z"), 0),
        );
        let game = "c:\\apps\\game.exe";

        // Active, then focused-but-idle past the threshold, then active again.
        handle(
            &ctx,
            report_of(vec![
                obs(game, "2026-08-25T10:00:00Z", 0),
                obs(game, "2026-08-25T10:00:30Z", 90),
            ]),
        );
        handle(&ctx, report_of(vec![obs(game, "2026-08-25T10:01:00Z", 90)]));
        // Activity resumes: a fresh run starts here.
        handle(
            &ctx,
            report_of(vec![
                obs(game, "2026-08-25T10:03:00Z", 0),
                obs(game, "2026-08-25T10:03:30Z", 0),
            ]),
        );

        let mut db_guard = lock_db(&ctx.db);
        // Only the resumed run bridges: [10:03:00, 10:03:30] = 30s. The idle
        // gap contributed nothing even though the app stayed foregrounded.
        assert_eq!(
            used_secs_for_key(&mut db_guard, "c:\\apps\\game.exe", DayKey(20260825)),
            30
        );
    }

    #[test]
    fn report_usage_skips_wildly_future_timestamps_but_still_accepts() {
        let ctx = test_ctx(
            Db::open_in_memory().expect("db"),
            TestClock::new(at("2026-08-25T10:05:00Z"), 0),
        );

        let response = handle(
            &ctx,
            report_of(vec![
                obs("c:\\apps\\game.exe", "2026-08-25T11:00:00Z", 0),
                obs("c:\\apps\\game.exe", "2026-08-25T10:04:50Z", 0),
            ]),
        );
        // One bad observation must not fail the batch.
        expect_accepted(response, at("2026-08-25T10:05:00Z"));

        let db_guard = lock_db(&ctx.db);
        assert_eq!(
            db_guard
                .day_summary(DayKey(20260825))
                .expect("summary")
                .total_seconds,
            0,
            "future-dated observations credit nothing (and the surviving \
             singleton observation credits no time on its own)"
        );
    }

    #[test]
    fn report_usage_dedupes_a_resent_batch_across_requests() {
        let ctx = test_ctx(
            Db::open_in_memory().expect("db"),
            TestClock::new(at("2026-08-25T10:05:00Z"), 0),
        );
        let batch = || {
            report_of(vec![
                obs("c:\\apps\\game.exe", "2026-08-25T10:00:00Z", 0),
                obs("c:\\apps\\game.exe", "2026-08-25T10:00:30Z", 0),
            ])
        };

        handle(&ctx, batch());
        // The reply to the first send was lost; the helper resends verbatim.
        handle(&ctx, batch());

        let mut db_guard = lock_db(&ctx.db);
        assert_eq!(
            used_secs_for_key(&mut db_guard, "c:\\apps\\game.exe", DayKey(20260825)),
            30,
            "a resent batch must be counted once"
        );
    }

    #[test]
    fn report_usage_splits_credit_at_the_local_day_boundary_mid_batch() {
        let ctx = test_ctx(
            Db::open_in_memory().expect("db"),
            TestClock::new(at("2026-08-21T00:05:00Z"), 0),
        );

        handle(
            &ctx,
            report_of(vec![
                obs("c:\\apps\\game.exe", "2026-08-20T23:59:50Z", 0),
                obs("c:\\apps\\game.exe", "2026-08-21T00:00:20Z", 0),
            ]),
        );

        let mut db_guard = lock_db(&ctx.db);
        assert_eq!(
            used_secs_for_key(&mut db_guard, "c:\\apps\\game.exe", DayKey(20260820)),
            10,
            "the old day keeps its share"
        );
        assert_eq!(
            used_secs_for_key(&mut db_guard, "c:\\apps\\game.exe", DayKey(20260821)),
            20,
            "the new day starts accruing at the boundary"
        );
    }

    #[test]
    fn report_usage_marks_tracking_available_in_status() {
        let ctx = test_ctx(
            Db::open_in_memory().expect("db"),
            TestClock::new(at("2026-08-25T10:05:00Z"), 0),
        );

        let Response::Status(before) = handle(&ctx, Request::Status) else {
            panic!("expected Status");
        };
        assert!(
            !before.tracking_available,
            "no reports and no self-sampling: tracking is down"
        );

        handle(
            &ctx,
            report_of(vec![obs("c:\\apps\\game.exe", "2026-08-25T10:04:50Z", 0)]),
        );

        let Response::Status(after) = handle(&ctx, Request::Status) else {
            panic!("expected Status");
        };
        assert!(
            after.tracking_available,
            "a fresh report means tracking works"
        );
    }

    /// Shared server boilerplate for the live-pipe tests.
    #[cfg(windows)]
    fn spawn_test_server(name: &'static str) -> IpcServerHandle {
        spawn(
            name,
            Arc::new(Mutex::new(Db::open_in_memory().expect("db"))),
            StatusInfo {
                agent_version: "test".into(),
                tracker_backend: "fake".into(),
                enforcement_backend: "fake".into(),
                filter_backend: "none".into(),
                self_sampling: false,
                blocks_encrypted_dns: false,
            },
            Policy {
                limit_cooldown_hours: 24,
                strict_mode: false,
                day_start_minutes: 0,
                idle_threshold_secs: 60,
                show_hud_overlay: true,
            },
            Arc::new(Mutex::new(Box::<FakeProcesses>::default())),
            Arc::new(TestClock::new(at("2026-08-20T12:00:00Z"), 0)),
        )
    }

    /// The concurrency contract: one connection, many frames. Legacy one-shot
    /// clients exercise the same worker loop with a single frame.
    #[test]
    #[cfg(windows)]
    fn a_persistent_connection_serves_many_frames_in_order() {
        let name: &'static str =
            Box::leak(format!("screentime_agent_test_{}", std::process::id()).into_boxed_str());
        let _server = spawn_test_server(name);

        let mut stream = st_ipc::transport::client_connect(name).expect("connect");
        for expected in ["pong", "status"] {
            let request = if expected == "pong" {
                Request::Ping
            } else {
                Request::Status
            };
            st_ipc::write_message(&mut stream, &request).expect("write");
            let response: Response = st_ipc::read_message(&mut stream).expect("read");
            match (&response, expected) {
                (Response::Pong, "pong") => {}
                (Response::Status(_), "status") => {}
                _ => panic!("frame {expected} answered with {response:?}"),
            }
        }
        drop(stream);

        // A legacy one-shot client still gets served after the persistent
        // connection above held a worker.
        let mut second = st_ipc::transport::client_connect(name).expect("reconnect");
        st_ipc::write_message(&mut second, &Request::Ping).expect("write");
        assert!(matches!(
            st_ipc::read_message::<_, Response>(&mut second),
            Ok(Response::Pong)
        ));
    }

    /// Malformed bytes must close that one connection without killing the
    /// server or poisoning anything shared.
    #[test]
    fn read_errors_surface_as_closed_or_codec_failures_only() {
        // Directly pins the worker-loop classification the server relies on:
        // clean EOF is Closed; garbage frames are codec errors, never panics.
        let empty: &[u8] = &[];
        assert!(matches!(
            st_ipc::read_message::<_, Request>(&mut std::io::Cursor::new(empty)),
            Err(IpcError::Closed)
        ));
    }

    /// Regression for the deaf-agent incident: every connection is served by a
    /// worker holding one slot, and before the SlotGuard fix a worker that
    /// died unexpectedly (panic path) skipped its release — leaking capacity
    /// until `acquire` blocked forever and no pipe instance ever existed
    /// again. Flooding well past [`MAX_WORKERS`] malformed connections must
    /// leave the server fully alive.
    #[test]
    #[cfg(windows)]
    fn a_flood_of_broken_connections_cannot_exhaust_the_worker_pool() {
        let name: &'static str = Box::leak(
            format!("screentime_agent_test_flood_{}", std::process::id()).into_boxed_str(),
        );
        spawn_test_server(name);

        use std::io::Write;
        for _ in 0..(MAX_WORKERS * 3) {
            let mut stream = st_ipc::transport::client_connect(name).expect("connect");
            // Well-formed length prefix, nonsense body: the frame layer rejects
            // it and the worker closes the connection.
            stream.write_all(&4u32.to_le_bytes()).expect("write len");
            stream.write_all(b"junk").expect("write body");
            drop(stream);
        }

        let mut probe = st_ipc::transport::client_connect(name).expect(
            "server must still be listening after far more broken connections than workers",
        );
        st_ipc::write_message(&mut probe, &Request::Ping).expect("write ping");
        assert!(matches!(
            st_ipc::read_message::<_, Response>(&mut probe),
            Ok(Response::Pong)
        ));
    }
}
