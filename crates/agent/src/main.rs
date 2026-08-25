//! `screentime-agent` — the privileged component.
//!
//! Owns the database, the limits engine and all enforcement. Runs as a Windows
//! Service or a systemd unit; during development it runs in the foreground.
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
mod enforcer;
mod ipc_server;
mod locks;
mod platform;
mod sampler;

use std::path::PathBuf;
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

use crate::enforcer::Enforcer;
use crate::ipc_server::{IpcServerHandle, StatusInfo};
use crate::locks::lock_db;
use crate::platform::Backends;
use crate::sampler::{PendingInterval, Sampler};

const POLL: StdDuration = StdDuration::from_secs(1);
const CLOCK_TOLERANCE_SECS: i64 = 3;

fn main() -> Result<()> {
    init_tracing();
    install_shutdown_handler();

    let data_dir = data_dir()?;
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating data directory {}", data_dir.display()))?;

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
// The handler itself must be async-signal-safe: it touches only an atomic.
// ---------------------------------------------------------------------------

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

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
                SHUTDOWN.store(true, Ordering::Relaxed);
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
        SHUTDOWN.store(true, Ordering::Relaxed);
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

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
