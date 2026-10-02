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
//! Anything that loosens enforcement is PIN-gated by [`auth`] (unless no PIN
//! is configured yet); loosening limit minutes is also subject to the
//! anti-impulse cooldown, while tightening applies immediately.
//!
//! # Layout
//!
//! This file holds the server (accept loop, worker cap, connection loop), the
//! shared state ([`Ctx`], [`Live`]) and dispatch. [`auth`] decides who may do
//! what before dispatch. Handlers live in child modules by area: `dashboard`
//! (read-only queries), `limits`, `vault` (PIN), `settings`, `downtime`,
//! `web`, `apps` and `ingest` (session-helper usage reports). Tests are in
//! `tests.rs`.
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
use st_core::pin::{generate_recovery_code, hash_pin, normalize_recovery_code, PinThrottle};
use st_core::platform::ProcessController;
use st_core::settings::SettingKey;
use st_ipc::{
    transport, ErrorCode, LimitTargetDto, ObservationDto, ReportUsageDto, Request, Response,
    StatusDto, UsageRowDto,
};
use st_storage::{Db, LimitRow};

use crate::family_dns::{self, DnsBackend};
use crate::locks::{lock_db, lock_recover, read_recover, write_recover};
pub use crate::policy::Policy;

mod apps;
mod auth;
mod dashboard;
mod downtime;
mod ingest;
mod limits;
mod settings;
#[cfg(test)]
mod tests;
mod vault;
mod web;

use crate::persist;
use crate::sampler::PendingInterval;
use apps::*;
use dashboard::*;
use downtime::*;
use ingest::*;
use limits::*;
use settings::*;
use vault::*;
use web::*;

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

/// Runtime-updated facts shared between IPC workers and the main loop.
///
/// Three small independent mutexes instead of one: handlers touch them one at
/// a time, and nested locking is a deadlock waiting to happen.
pub struct Live {
    last_report: Mutex<Option<DateTime<Utc>>>,
    focus: Mutex<Option<(AppKey, DateTime<Utc>)>>,
    seen_observations: Mutex<Vec<(String, i64)>>,
    open_chains: Mutex<HashMap<String, ChainState>>,
    /// Brute-force throttle shared by every credential check; see `auth`.
    pin_throttle: Mutex<PinThrottle>,
}

impl Default for Live {
    fn default() -> Self {
        Self {
            last_report: Mutex::new(None),
            focus: Mutex::new(None),
            seen_observations: Mutex::new(Vec::new()),
            open_chains: Mutex::new(HashMap::new()),
            pin_throttle: Mutex::new(PinThrottle::default()),
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

    /// Whether the session helper reported within `secs` (it reports every
    /// second, empty batches included). Read by the helper supervisor.
    pub(crate) fn helper_reported_within(&self, secs: i64, now: DateTime<Utc>) -> bool {
        self.reported_within(secs, now)
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
    /// Family DNS adapter configuration; a fake in tests.
    dns: Arc<dyn DnsBackend>,
    /// Where the DNS backup file lives. `None` in tests.
    data_dir: Option<std::path::PathBuf>,
}

/// Everything [`spawn`] needs from the agent's startup.
pub struct ServerDeps {
    pub db: Arc<Mutex<Db>>,
    pub status: StatusInfo,
    /// Shared with the main loop, which reads it every tick.
    pub policy: Arc<std::sync::RwLock<Policy>>,
    pub processes: Arc<Mutex<Box<dyn ProcessController>>>,
    pub clock: Arc<dyn Clock>,
    pub dns: Arc<dyn DnsBackend>,
    pub data_dir: Option<std::path::PathBuf>,
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
pub fn spawn(pipe_name: &'static str, deps: ServerDeps) -> IpcServerHandle {
    let live = Arc::new(Live::default());
    let ctx = Ctx {
        db: deps.db,
        status: Arc::new(deps.status),
        policy: deps.policy,
        processes: deps.processes,
        clock: deps.clock,
        live: live.clone(),
        dns: deps.dns,
        data_dir: deps.data_dir,
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
    let peer = auth::Peer::of(&stream);
    if peer == auth::Peer::Unknown {
        tracing::debug!("IPC peer process could not be inspected");
    }
    loop {
        match st_ipc::read_message::<_, Request>(&mut stream) {
            Ok(request) => {
                let response = handle_from(&ctx, peer, request);
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

/// Dispatch a request from a trusted in-process caller (tests).
#[cfg(test)]
fn handle(ctx: &Ctx, request: Request) -> Response {
    handle_from(ctx, auth::Peer::SessionHelper, request)
}

fn handle_from(ctx: &Ctx, peer: auth::Peer, request: Request) -> Response {
    if let Err(denied) = auth::authorize(ctx, peer, &request) {
        return denied.into();
    }
    let now = ctx.clock.now_utc();
    match request {
        Request::Ping => Response::Pong,
        Request::Status => status_response(ctx),
        Request::DaySummary { day } => day_summary(ctx, day),
        Request::WeeklySummary { end_day } => weekly_summary(ctx, end_day),
        Request::Catalog => catalog(ctx),
        Request::BlockedApps => blocked_apps(ctx),
        Request::CloseApps { app_id, .. } => close_apps(ctx, app_id),
        Request::SetPin {
            new_pin,
            current_pin,
        } => set_pin(ctx, &new_pin, current_pin.as_deref()),
        Request::RecoverPin {
            recovery_code,
            new_pin,
        } => recover_pin(ctx, &recovery_code, &new_pin),
        Request::RemovePin { credential } => remove_pin(ctx, &credential),
        Request::SetSetting { key, value, .. } => set_setting(ctx, &key, &value),
        Request::SetLimit {
            target,
            default_minutes,
            weekday_minutes,
            enabled,
            ..
        } => set_limit(
            ctx,
            LimitSpec {
                target,
                default_minutes,
                weekday_minutes,
                enabled,
            },
            now,
        ),
        Request::DeleteLimit { target, .. } => delete_limit(ctx, target, now),
        Request::CancelPendingLimit { target, .. } => cancel_pending_limit(ctx, target),
        Request::GrantOverride {
            target, seconds, ..
        } => grant_override(ctx, target, seconds, now),
        Request::Categorize {
            app_id,
            primary,
            tags,
            ..
        } => categorize(ctx, app_id, primary, &tags),
        Request::RegisterDiscoveredApps { apps } => register_discovered_apps(ctx, apps, now),
        Request::ReportUsage { report } => report_usage(ctx, report),
        // `auth` already verified the PIN; reaching here means it matched.
        Request::VerifyPin { .. } => accepted(now),
        Request::ListManualBlocks => list_manual_blocks(ctx),
        Request::AddManualBlock { domain } => add_manual_block(ctx, &domain),
        Request::RemoveManualBlock { domain, .. } => remove_manual_block(ctx, &domain),
        Request::ListSchedules => list_schedules(ctx),
        Request::CreateSchedule {
            name,
            weekday_mask,
            start_minute,
            end_minute,
        } => create_schedule(ctx, &name, weekday_mask, start_minute, end_minute),
        Request::UpdateSchedule {
            id,
            name,
            weekday_mask,
            start_minute,
            end_minute,
            ..
        } => update_schedule(ctx, id, &name, weekday_mask, start_minute, end_minute),
        Request::SetScheduleEnabled { id, enabled, .. } => set_schedule_enabled(ctx, id, enabled),
        Request::DeleteSchedule { id, .. } => delete_schedule(ctx, id),
        Request::ListAllowlist => list_allowlist(ctx),
        Request::SetAllowlist {
            subject_type,
            subject_id,
            allowed,
            ..
        } => set_allowlist(ctx, &subject_type, subject_id, allowed),
    }
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
