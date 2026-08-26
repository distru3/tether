//! `screentime-agent` — the privileged component.
//!
//! Owns the database, the limits engine and all enforcement. Runs as a Windows
//! Service (`--service`, registered via `--install`) or in the foreground
//! console mode; see [`cli`] for the full command surface.
//!
//! # Where usage comes from (M2 architecture)
//!
//! The unprivileged session helper samples the desktop at ~1 Hz and reports
//! collapsed observations over IPC ([`st_ipc::Request::ReportUsage`]);
//! ingestion, classification, storage, limits and blocking all live here. The
//! agent's own 1 Hz sampling loop is a *development fallback*, enabled only
//! when `SCREENTIME_SELF_SAMPLE=1`, so it can never double-count against
//! helper reports. [`st_ipc::StatusDto::tracking_available`] is honest about
//! which path is alive: fresh reports count, otherwise only an active fallback
//! does.
//!
//! Enforcement stays live without local sampling: every accepted report
//! updates the shared "last focused app" fact, which the evaluation tick feeds
//! into the enforcer exactly as the old sampler did.

mod classify;
mod cli;
mod enforcer;
mod ipc_server;
mod locks;
mod platform;
mod sampler;
mod service;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use st_core::clock::{Clock, ClockGuard, ClockVerdict, SystemClock};
use st_core::daykey::DayKey;
use st_core::limits::LimitEngine;
use st_core::model::SubjectRef;
use st_core::platform::IdleState;
use st_ipc::PIPE_NAME;
use st_storage::Db;
use tracing_appender::non_blocking::WorkerGuard;
// SubscriberExt/Layer for stacking the two-layer registry in init_tracing.
use tracing_subscriber::prelude::*;

use crate::enforcer::Enforcer;
use crate::ipc_server::{IpcServerHandle, StatusInfo};
use crate::locks::lock_db;
use crate::platform::Backends;
use crate::sampler::{PendingInterval, Sampler};

const POLL: StdDuration = StdDuration::from_secs(1);
const CLOCK_TOLERANCE_SECS: i64 = 3;

/// Exit status used exclusively for "another agent instance is already
/// running". Distinct from generic failure (1) so supervisors and scripts can
/// tell "port already taken by myself" apart from real startup errors. The
/// service wrapper forwards it as the Stopped exit code, keeping `sc query`
/// diagnosable too.
pub(crate) const EXIT_ALREADY_RUNNING: i32 = 2;

/// Kernel name of the single-instance guard mutex.
///
/// `Global\` on purpose: the agent is one per MACHINE (it owns the database
/// under ProgramData and serves `\\.\pipe\screentime`), so a second agent
/// started inside another user's login session must still lose — a `Local\`
/// name would let both run and fight over the pipe. Interactive users may
/// create `Global\` objects without privilege ceremony here, and the mutex
/// dies with its owning process, so a crash can never wedge future startups.
const SINGLE_INSTANCE_MUTEX: &str = r"Global\screentime-agent";

/// How many daily log files to keep before the oldest is deleted. Bounded on
/// purpose: an unattended service writing dailies forever would eventually
/// fill the disk it is supposed to be protecting.
const MAX_LOG_FILES: usize = 14;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::decide(&args) {
        Ok(cli::Action::Console) => run_console(),
        Ok(cli::Action::RunAsService) => service::run_as_service(),
        Ok(cli::Action::Install) => service::install(),
        Ok(cli::Action::Uninstall) => service::uninstall(),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}

/// Console mode: byte-for-byte the historical foreground daemon. The
/// single-instance guard runs before anything else (tracing included) — see
/// [`SINGLE_INSTANCE_MUTEX`] for why it must win every race.
///
/// The guard binding lives to the end of the function on purpose: dropping it
/// would release the mutex mid-process and re-enable a second daemon.
fn run_console() -> Result<()> {
    // Two agents would fight over \\.\pipe\screentime in ways that surface as
    // silent weirdness, not errors. The subscriber does not exist yet at this
    // point, so the refusal is printed straight to stderr instead of logged.
    #[cfg(windows)]
    let _single_instance = match try_acquire_single_instance() {
        Ok(guard) => Some(guard),
        Err(st_win32::AlreadyRunning) => {
            eprintln!(
                "error: another screentime-agent is already running ({SINGLE_INSTANCE_MUTEX} held); exiting"
            );
            std::process::exit(EXIT_ALREADY_RUNNING);
        }
    };

    run_daemon("console")
}

/// Non-dying single-instance acquisition for callers that must report failure
/// through their own channel: [`run_console`] exits with
/// [`EXIT_ALREADY_RUNNING`], while the service path reports Stopped carrying
/// that code so `sc query` stays diagnosable.
#[cfg(windows)]
pub(crate) fn try_acquire_single_instance(
) -> Result<st_win32::OwnedMutexHandle, st_win32::AlreadyRunning> {
    st_win32::acquire_single_instance(SINGLE_INSTANCE_MUTEX)
}

/// THE startup-and-run path, shared by console mode and the Windows service
/// ([`service`]): data dir, tracing, shutdown handlers, database, backends,
/// IPC server and the 1 Hz loop. Both modes must stay byte-identical here —
/// the service wrapper only wraps status reporting around this call.
///
/// Why the console ctrl handler is installed unconditionally: under a service
/// there is no console to deliver events to, so the handler simply never
/// fires — keeping ONE startup path beats splitting it per launch mode. The
/// service's own stop control flips the same [`request_shutdown`] flag.
fn run_daemon(mode_label: &'static str) -> Result<()> {
    let data_dir = data_dir()?;
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating data directory {}", data_dir.display()))?;

    // Hold the returned WorkerGuard for the whole process (bound in `main`);
    // see init_tracing for why letting it die early silently kills file logs.
    let _log_guard = init_tracing(&agent_log_dir(&data_dir));
    install_shutdown_handler();
    tracing::info!(mode = mode_label, "agent entrypoint reached");

    let db_path = data_dir.join("screentime.db");
    let db =
        Arc::new(Mutex::new(Db::open(&db_path).with_context(|| {
            format!("opening database {}", db_path.display())
        })?));
    tracing::info!(path = %db_path.display(), "database ready");

    let clock = Arc::new(SystemClock::new());
    // Read the policy knobs one statement each: a temporary `MutexGuard` lives
    // until the end of its statement, so two lock acquisitions inside a single
    // struct literal would deadlock on the second one.
    let day_start_minutes = lock_db(&db).setting_i64("day_start_minutes", 0);
    let idle_threshold = lock_db(&db).setting_i64("idle_threshold_secs", 60).max(1) as u64;
    let capture_titles = lock_db(&db)
        .setting("capture_window_titles")?
        .map(|v| v == "true")
        .unwrap_or(false);

    let mut backends = platform::detect(capture_titles, idle_threshold);
    tracing::info!(
        tracker = backends.tracker.backend(),
        idle = backends.idle.backend(),
        processes = backends
            .processes
            .as_ref()
            .map(|p| p.backend())
            .unwrap_or("none"),
        filter = backends.filter.backend(),
        "backends selected"
    );

    let uncategorized = {
        let guard = lock_db(&db);
        guard
            .category_id(st_core::category::UNCATEGORIZED_SLUG)
            .context("uncategorized category missing; database seed failed")?
    };

    // Development fallback only: see the module docs for why this is opt-in.
    let self_sampling = std::env::var("SCREENTIME_SELF_SAMPLE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let status = StatusInfo {
        agent_version: env!("CARGO_PKG_VERSION").to_string(),
        tracker_backend: backends.tracker.backend().to_string(),
        enforcement_backend: backends
            .processes
            .as_ref()
            .map(|p| p.backend().to_string())
            .unwrap_or_else(|| "none".into()),
        filter_backend: backends.filter.backend().to_string(),
        self_sampling,
        blocks_encrypted_dns: false,
    };

    // The process controller is no longer used by the sampler (M2.5 removed
    // the freeze); only the IPC server needs it, for the overlay's "Quit"
    // action. Move it out of the backends and share it behind a lock.
    let processes = Arc::new(Mutex::new(
        backends
            .processes
            .take()
            .expect("process controller present"),
    ));

    let limit_cooldown_hours = lock_db(&db).setting_i64("limit_cooldown_hours", 24);
    let strict_mode = lock_db(&db)
        .setting("strict_mode")
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false);

    let ipc = ipc_server::spawn(
        PIPE_NAME,
        db.clone(),
        status,
        ipc_server::Policy {
            limit_cooldown_hours,
            strict_mode,
            day_start_minutes,
            idle_threshold_secs: idle_threshold as i64,
        },
        processes,
        clock.clone(),
    );
    tracing::info!(pipe = PIPE_NAME, "IPC server listening");

    run_main_loop(
        clock,
        db,
        uncategorized,
        day_start_minutes,
        self_sampling,
        backends,
        ipc,
    );

    tracing::info!("shutdown complete");
    Ok(())
}

/// The sampling/enforcement loop. Returns on graceful shutdown instead of
/// exiting in place so `main` can log a clean stop.
fn run_main_loop(
    clock: Arc<dyn Clock>,
    db: Arc<Mutex<Db>>,
    uncategorized: i64,
    day_start_minutes: i64,
    self_sampling: bool,
    mut backends: Backends,
    ipc: IpcServerHandle,
) {
    // Only the development fallback owns a sampler; report ingestion replaces
    // it otherwise.
    let mut sampler = self_sampling.then(|| Sampler::new(day_start_minutes));
    let mut guard = ClockGuard::new(&*clock, Duration::seconds(CLOCK_TOLERANCE_SECS));
    let mut enforcer = Enforcer::new();
    let mut tracker_error_logged = false;

    tracing::info!("agent running; press Ctrl+C to stop");

    while !shutdown_requested() {
        let now = clock.now_utc();
        let tz_offset = clock.local_offset_seconds();
        let verdict = guard.check(&*clock);

        if let ClockVerdict::Backward { by } = verdict {
            tracing::warn!(seconds = by.num_seconds(), "system clock moved backwards");
            let _ = lock_db(&db).audit(
                now,
                "clock_backward",
                Some(&format!("{} seconds", by.num_seconds())),
            );
        }

        // Sampling consumes local OS events only when the dev fallback runs;
        // otherwise session-helper reports have already updated shared focus
        // state by the time the evaluation tick reads it.
        if let Some(sampler) = sampler.as_mut() {
            let idle = backends.idle.idle_state().unwrap_or(IdleState::Locked);
            let window = match backends.tracker.active_window() {
                Ok(window) => window,
                Err(e) => {
                    if !tracker_error_logged {
                        tracing::error!(error = %e, "active-window tracking unavailable");
                        tracker_error_logged = true;
                    }
                    None
                }
            };
            for pending in sampler.observe(now, tz_offset, window.as_ref(), idle, verdict) {
                persist_pending(&db, &pending, uncategorized, now);
            }
        }

        // Every tick evaluates limits: a handful of indexed reads over local
        // SQLite (sub-millisecond), and 1 Hz is what makes enforcement feel
        // live — a crossed limit blocks within a second of the credit landing.
        evaluate_limits(
            &*clock,
            &db,
            &ipc,
            &mut enforcer,
            now,
            tz_offset,
            day_start_minutes,
        );

        std::thread::sleep(POLL);
    }

    // Graceful shutdown: credit the final interval before the process dies,
    // otherwise the last stretch of every session would vanish on Ctrl+C.
    if let Some(sampler) = sampler.as_mut() {
        if let Some(pending) = sampler.flush() {
            tracing::info!(
                app = %pending.key,
                seconds = pending.duration_secs(),
                "flushing final interval on shutdown"
            );
            persist_pending(&db, &pending, uncategorized, clock.now_utc());
        }
    }
}

fn evaluate_limits(
    clock: &dyn Clock,
    db: &Arc<Mutex<Db>>,
    ipc: &IpcServerHandle,
    enforcer: &mut Enforcer,
    now: DateTime<Utc>,
    tz_offset: i32,
    day_start_minutes: i64,
) {
    let today = DayKey::from_utc(now, tz_offset, day_start_minutes);

    let limits = {
        let mut db_guard = lock_db(db);
        // A loosened limit may have come into effect since the last tick.
        if let Err(e) = db_guard.promote_pending_limits(now) {
            tracing::warn!(error = %e, "promoting pending limits failed");
        }
        match db_guard.load_limits() {
            Ok(limits) => limits,
            Err(e) => {
                // An unreadable table must not look like "no limits": say so
                // loudly. Enforcement continues on whatever was read last —
                // here nothing — while edits over IPC fail closed too.
                tracing::error!(error = %e, "loading limits failed; enforcing empty set this tick");
                Vec::new()
            }
        }
    };
    let engine = LimitEngine::with_default_warnings(limits);

    // With sampling moved to the session helper, the focused app arrives via
    // reported observations; nothing to evaluate when reports went quiet.
    let focus = ipc.live.focus_key(clock.now_utc());
    let result = enforcer.tick(
        &mut lock_db(db),
        &engine,
        focus.as_ref(),
        now,
        tz_offset,
        day_start_minutes,
    );

    if let Err(e) = result {
        tracing::error!(error = %e, "limit enforcement failed");
    }
    let _ = today;
}

/// Persist one completed interval, auto-classifying the app on first sight.
///
/// Shared by the dev-fallback sampler and report ingestion so both paths
/// produce identical rows.
fn persist(
    db: &mut Db,
    pending: &PendingInterval,
    default_category: i64,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    let app_id = db.upsert_app(
        &pending.key,
        &pending.display_name,
        None,
        default_category,
        now,
    )?;

    // Auto-classify an app the first time it is seen: only when it still sits in
    // the default (uncategorized) bucket and no human has overridden it. The
    // check is cheap and idempotent, so already-classified apps are untouched.
    let (primary, user_classified) = db.app_category_state(app_id)?;
    if !user_classified && primary == default_category {
        if let Some(classification) = classify::classify(&pending.key) {
            let primary_id = db.category_id(classification.primary)?;
            let mut tags = Vec::with_capacity(classification.tags.len());
            for tag in classification.tags {
                tags.push(db.category_id(tag)?);
            }
            db.set_app_categories(app_id, primary_id, &tags, false)?;
            tracing::debug!(
                app = %pending.key,
                primary = classification.primary,
                "auto-classified app"
            );
        }
    }

    db.record_interval(&st_core::model::UsageInterval {
        subject: SubjectRef::App(app_id),
        session_id: "default".into(),
        start: pending.start,
        end: pending.end,
        day_key: pending.day,
    })?;

    tracing::debug!(
        app = %pending.key,
        seconds = pending.duration_secs(),
        day = %pending.day,
        "recorded interval"
    );
    Ok(())
}

fn persist_pending(
    db: &Arc<Mutex<Db>>,
    pending: &PendingInterval,
    default_category: i64,
    now: DateTime<Utc>,
) {
    if let Err(e) = persist(&mut lock_db(db), pending, default_category, now) {
        tracing::error!(error = %e, app = %pending.key, "failed to persist interval");
    }
}

// ---------------------------------------------------------------------------
// Graceful shutdown.
//
// The loop polls a flag that OS stop events set, then flushes the final
// sampling interval before exiting. Without this, Ctrl+C killed the process
// mid-interval and the last stretch of usage was simply lost (`Sampler::flush`
// existed but was unreachable dead code).
//
// The handlers themselves must be async-signal-safe: they touch only an
// atomic. Both stop families — console ctrl events / Unix signals AND the
// Windows service STOP control — funnel through [`request_shutdown`] so there
// is exactly ONE flag and one graceful tail for every launch mode.
// ---------------------------------------------------------------------------

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Ask the main loop to exit gracefully at its next tick.
///
/// Called from the console ctrl handler, the Unix signal handlers and the
/// service control handler; async-signal-safe by construction (one atomic
/// store).
pub(crate) fn request_shutdown() {
    SHUTDOWN.store(true, Ordering::Relaxed);
}

fn shutdown_requested() -> bool {
    SHUTDOWN.load(Ordering::Relaxed)
}

#[cfg(windows)]
fn install_shutdown_handler() {
    use windows::Win32::Foundation::BOOL;
    use windows::Win32::System::Console::{
        SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT, CTRL_LOGOFF_EVENT,
        CTRL_SHUTDOWN_EVENT,
    };

    unsafe extern "system" fn on_ctrl_event(ctrl_type: u32) -> BOOL {
        match ctrl_type {
            CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT
            | CTRL_SHUTDOWN_EVENT => {
                request_shutdown();
                BOOL(1)
            }
            _ => BOOL(0),
        }
    }

    let installed = unsafe { SetConsoleCtrlHandler(Some(on_ctrl_event), true) };
    if let Err(e) = installed {
        // Not fatal: the process still stops on Ctrl+C, just without flushing.
        tracing::warn!(error = %e, "could not install console ctrl handler; shutdown will not be graceful");
    }
}

#[cfg(unix)]
fn install_shutdown_handler() {
    use nix::sys::signal::{signal, SigHandler, Signal};

    unsafe extern "C" fn on_signal(_sig: i32) {
        request_shutdown();
    }

    for sig in [Signal::SIGINT, Signal::SIGTERM] {
        let handler = unsafe { signal(sig, SigHandler::Handler(on_signal)) };
        if let Err(e) = handler {
            tracing::warn!(error = %e, ?sig, "could not install signal handler");
        }
    }
}

fn data_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("SCREENTIME_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }

    #[cfg(windows)]
    {
        let base = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".to_string());
        Ok(PathBuf::from(base).join("screentime"))
    }

    #[cfg(not(windows))]
    {
        Ok(PathBuf::from("/var/lib/screentime"))
    }
}

/// The agent's rolling-log home: `<data_dir>/logs`, next to the database it
/// diagnoses. Keeping logs beside the data dir means one `SCREENTIME_DATA_DIR`
/// override relocates an entire incident bundle.
fn agent_log_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

/// Installs the tracing stack: one filter, two destinations.
///
/// # Why two layers
///
/// Console output serves interactive development; a daily-rolling file serves
/// incidents — the agent usually runs hidden or as a Windows service, where
/// stdout is discarded by whoever spawned it and post-mortem diagnosis would
/// otherwise have nothing to read. Both layers share ONE `RUST_LOG`-derived
/// [`tracing_subscriber::EnvFilter`], placed at the top of the registry stack
/// so it governs every layer below: the file sees exactly what the console
/// would.
///
/// # The classic tracing-appender trap (why this returns a guard)
///
/// `non_blocking` hands back a [`WorkerGuard`] owning the background writer
/// thread. Dropping that guard shuts the thread down and can lose buffered
/// lines — most commonly by letting it die inside the init function as a
/// temporary. `main` must therefore hold the return value for the whole
/// process lifetime.
///
/// A broken log sink must never take the tracker down: if the log directory
/// cannot be created or opened, this falls back to console-only logging with
/// a loud warning instead of failing startup.
fn init_tracing(log_dir: &Path) -> Option<WorkerGuard> {
    // Built once for both layers (see doc above); RUST_LOG honoured, "info"
    // when unset — unchanged from the previous console-only setup.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    match build_daily_appender(log_dir, "screentime-agent.log") {
        Ok(appender) => {
            // Non-blocking on purpose: a slow or full disk must never stall
            // the 1 Hz enforcement tick behind a logging syscall.
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let stdout_layer = tracing_subscriber::fmt::layer().with_target(false);
            let file_layer = tracing_subscriber::fmt::layer()
                .with_target(false)
                // ANSI colour codes belong on terminals, not incident logs.
                .with_ansi(false)
                .with_writer(writer);
            tracing_subscriber::registry()
                .with(filter)
                .with(stdout_layer)
                .with(file_layer)
                .init();
            Some(guard)
        }
        Err(e) => {
            // Console-only fallback, identical to the pre-file-logging setup;
            // only after `.init()` does the warning below actually surface.
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_target(false)
                .init();
            tracing::warn!(
                error = %e,
                dir = %log_dir.display(),
                "file logging unavailable; continuing with console output only"
            );
            None
        }
    }
}

/// Creates `log_dir` and opens a daily-rolling appender named `base_name`.
///
/// Uses the builder form deliberately: the `rolling::daily` convenience
/// constructor panics on an unusable directory, and startup must survive
/// that (see init_tracing's fallback contract).
fn build_daily_appender(
    log_dir: &Path,
    base_name: &str,
) -> anyhow::Result<tracing_appender::rolling::RollingFileAppender> {
    // Directory creation as its own step so the common failure (missing
    // parent) names the directory rather than surfacing as an open error.
    std::fs::create_dir_all(log_dir)
        .with_context(|| format!("creating log directory {}", log_dir.display()))?;
    tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        // The base name carries the extension so each day lands as
        // `screentime-agent.log.<yyyy-mm-dd>` — globbing `*.log.*` finds every
        // rotation while today's file still reads as screentime-agent.log.NN.
        .filename_prefix(base_name)
        .max_log_files(MAX_LOG_FILES)
        .build(log_dir)
        .with_context(|| {
            format!(
                "opening daily rolling log {base_name} in {}",
                log_dir.display()
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_log_dir_is_logs_under_the_data_dir() {
        assert_eq!(
            agent_log_dir(Path::new(r"C:\ProgramData\screentime")),
            PathBuf::from(r"C:\ProgramData\screentime").join("logs")
        );
    }
}
