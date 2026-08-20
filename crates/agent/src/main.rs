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

mod platform;
mod sampler;

use std::path::PathBuf;
use std::time::Duration as StdDuration;

use anyhow::{Context, Result};
use chrono::Duration;
use st_core::category::UNCATEGORIZED_SLUG;
use st_core::clock::{Clock, ClockGuard, ClockVerdict, SystemClock};
use st_core::daykey::DayKey;
use st_core::limits::{Decision, LimitEngine};
use st_core::model::{SubjectRef, UsageInterval};
use st_core::platform::IdleState;
use st_storage::Db;

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
    let mut db = Db::open(&db_path)
        .with_context(|| format!("opening database {}", db_path.display()))?;
    tracing::info!(path = %db_path.display(), "database ready");

    let clock = SystemClock::new();
    let day_start_minutes = db.setting_i64("day_start_minutes", 0);
    let idle_threshold = db.setting_i64("idle_threshold_secs", 60).max(1) as u64;
    let capture_titles = db
        .setting("capture_window_titles")?
        .map(|v| v == "true")
        .unwrap_or(false);

    let mut backends = platform::detect(capture_titles, idle_threshold);
    tracing::info!(
        tracker = backends.tracker.backend(),
        idle = backends.idle.backend(),
        processes = backends.processes.backend(),
        filter = backends.filter.backend(),
        "backends selected"
    );

    let uncategorized = db
        .category_id(UNCATEGORIZED_SLUG)
        .context("uncategorized category missing; database seed failed")?;

    let mut sampler = Sampler::new(day_start_minutes);
    let mut guard = ClockGuard::new(&clock, Duration::seconds(CLOCK_TOLERANCE_SECS));
    let mut ticks: u32 = 0;
    let mut tracker_error_logged = false;

    tracing::info!("agent running; press Ctrl+C to stop");

    loop {
        let now = clock.now_utc();
        let tz_offset = clock.local_offset_seconds();
        let verdict = guard.check(&clock);

        if let ClockVerdict::Backward { by } = verdict {
            tracing::warn!(seconds = by.num_seconds(), "system clock moved backwards");
            let _ = db.audit(
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
            if let Err(e) = persist(&mut db, &pending, uncategorized, now) {
                tracing::error!(error = %e, app = %pending.key, "failed to persist interval");
            }
        }

        ticks = ticks.wrapping_add(1);
        if ticks % EVALUATE_EVERY_TICKS == 0 {
            let today = DayKey::from_utc(now, tz_offset, day_start_minutes);
            if let Err(e) = report_limits(&db, today) {
                tracing::error!(error = %e, "limit evaluation failed");
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

fn report_limits(db: &Db, day: DayKey) -> Result<()> {
    let limits = db.load_limits()?;
    if limits.is_empty() {
        return Ok(());
    }

    let engine = LimitEngine::with_default_warnings(limits);
    let snapshot = db.day_snapshot(day)?;
    let Some(weekday) = day.weekday_index() else {
        anyhow::bail!("invalid day key {day}");
    };

    for limit in engine.limits() {
        let decision = match limit.target {
            st_core::limits::LimitTarget::App(app_id) => {
                engine.evaluate(app_id, &[], true, weekday, &snapshot)
            }
            _ => continue,
        };

        match decision {
            Decision::Block { binding } => {
                tracing::info!(?binding, "limit exhausted (enforcement lands in M2)")
            }
            Decision::Warn {
                remaining_secs,
                threshold_secs,
                ..
            } => tracing::info!(remaining_secs, threshold_secs, "approaching limit"),
            Decision::Allow { .. } => {}
        }
    }
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
