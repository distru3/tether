//! `screentime-agent` — the privileged component.
//!
//! Owns the database, the limits engine and all enforcement. Runs as a Windows
//! Service or a systemd unit; during development it runs in the foreground.
//!
//! # Current scope (M1)
//!
//! Tracking and reporting only. The limits engine is evaluated and its verdict
//! logged, but nothing is frozen yet: enforcement lands in M2, once tracking has
//! been dogfooded long enough to trust. Blocking apps based on numbers you have
//! not yet verified is how you end up locking yourself out of your own machine.

mod classify;
mod enforcer;
mod ipc_server;
mod platform;
mod sampler;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use anyhow::{Context, Result};
use chrono::Duration;
use st_core::category::UNCATEGORIZED_SLUG;
use st_core::clock::{Clock, ClockGuard, ClockVerdict, SystemClock};
use st_core::daykey::DayKey;
use st_core::limits::LimitEngine;
use st_core::model::{SubjectRef, UsageInterval};
use st_core::platform::IdleState;
use st_storage::Db;

use crate::enforcer::Enforcer;
use crate::sampler::{PendingInterval, Sampler};

const POLL: StdDuration = StdDuration::from_secs(1);
const EVALUATE_EVERY_TICKS: u32 = 5;
const CLOCK_TOLERANCE_SECS: i64 = 3;

fn main() -> Result<()> {
    init_tracing();

    let data_dir = data_dir()?;
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating data directory {}", data_dir.display()))?;

    let db_path = data_dir.join("screentime.db");
    let db =
        Arc::new(Mutex::new(Db::open(&db_path).with_context(|| {
            format!("opening database {}", db_path.display())
        })?));
    tracing::info!(path = %db_path.display(), "database ready");

    let clock = SystemClock::new();
    let day_start_minutes = db.lock().unwrap().setting_i64("day_start_minutes", 0);
    let idle_threshold = db
        .lock()
        .unwrap()
        .setting_i64("idle_threshold_secs", 60)
        .max(1) as u64;
    let capture_titles = db
        .lock()
        .unwrap()
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

    let uncategorized = db
        .lock()
        .unwrap()
        .category_id(UNCATEGORIZED_SLUG)
        .context("uncategorized category missing; database seed failed")?;

    // Answer the UI on a background thread; the sampler keeps the main loop.
    let status = ipc_server::StatusInfo {
        agent_version: env!("CARGO_PKG_VERSION").to_string(),
        tracker_backend: backends.tracker.backend().to_string(),
        enforcement_backend: backends
            .processes
            .as_ref()
            .map(|p| p.backend().to_string())
            .unwrap_or_else(|| "none".into()),
        filter_backend: backends.filter.backend().to_string(),
        // Hosts-file filtering cannot intercept DoH/DoT; be honest about it.
        tracking_available: true,
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
    // Read the policy knobs first, each in its own statement: a temporary
    // `MutexGuard` lives until the end of its statement, so two `db.lock()`
    // calls inside a single `Policy { .. }` literal would deadlock on the
    // second one.
    let limit_cooldown_hours = db.lock().unwrap().setting_i64("limit_cooldown_hours", 24);
    let strict_mode = db
        .lock()
        .unwrap()
        .setting("strict_mode")
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false);
    let _ipc_thread = ipc_server::spawn(
        db.clone(),
        status,
        ipc_server::Policy {
            limit_cooldown_hours,
            strict_mode,
        },
        processes,
    );
    tracing::info!(pipe = ipc_server::PIPE_NAME, "IPC server listening");

    let mut sampler = Sampler::new(day_start_minutes);
    let mut guard = ClockGuard::new(&clock, Duration::seconds(CLOCK_TOLERANCE_SECS));
    let mut enforcer = Enforcer::new();
    let mut ticks: u32 = 0;
    let mut tracker_error_logged = false;

    tracing::info!("agent running; press Ctrl+C to stop");

    loop {
        let now = clock.now_utc();
        let tz_offset = clock.local_offset_seconds();
        let verdict = guard.check(&clock);

        if let ClockVerdict::Backward { by } = verdict {
            tracing::warn!(seconds = by.num_seconds(), "system clock moved backwards");
            let _ = db.lock().unwrap().audit(
                now,
                "clock_backward",
                Some(&format!("{} seconds", by.num_seconds())),
            );
        }

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
            if let Err(e) = persist(&mut db.lock().unwrap(), &pending, uncategorized, now) {
                tracing::error!(error = %e, app = %pending.key, "failed to persist interval");
            }
        }

        ticks = ticks.wrapping_add(1);
        if ticks % EVALUATE_EVERY_TICKS == 0 {
            let today = DayKey::from_utc(now, tz_offset, day_start_minutes);
            let weekday = today.weekday_index().unwrap_or(0);

            let mut db_guard = db.lock().unwrap();
            // A loosened limit may have come into effect since the last tick.
            if let Err(e) = db_guard.promote_pending_limits(now) {
                tracing::warn!(error = %e, "promoting pending limits failed");
            }
            // Reload limits so tightening and freshly-promoted loosening apply.
            let engine =
                LimitEngine::with_default_warnings(db_guard.load_limits().unwrap_or_default());
            let result = enforcer.tick(
                &mut db_guard,
                &engine,
                window.as_ref().map(|w| &w.key),
                today,
                weekday,
            );
            drop(db_guard);
            if let Err(e) = result {
                tracing::error!(error = %e, "limit enforcement failed");
            }
        }

        std::thread::sleep(POLL);
    }
}

fn persist(
    db: &mut Db,
    pending: &PendingInterval,
    default_category: i64,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
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

    db.record_interval(&UsageInterval {
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
