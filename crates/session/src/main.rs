#![windows_subsystem = "windows"]
//! `screentime-session` — the per-login-session helper, and the workspace's
//! **sampling front**.
//!
//! # Why this binary exists
//!
//! On Windows, a service running under `LocalSystem` lives in Session 0 and
//! cannot see the interactive desktop, so `GetForegroundWindow` returns nothing
//! useful and it cannot display UI. Anything that needs the user's screen —
//! sampling focus *or* drawing over it — must live in a process started inside
//! the login session. That is this helper. It samples locally and reports what
//! it saw to the privileged agent over the named pipe; the agent stays the
//! only component that touches the database.
//!
//! # Wire behaviour: one persistent connection, capped-backoff reconnects
//!
//! The codec supports unlimited request/response pairs per connection, so the
//! helper connects once and keeps the stream open. Any failure on any frame
//! marks the whole link dead (framing state after an error is unknowable);
//! the stream is dropped and reconnects follow a capped exponential backoff
//! ([`backoff`]), reset by one healthy exchange. This stays correct whether
//! the agent serves many exchanges per connection or closes after each one.
//!
//! # Exact per-cycle sequence (~1 Hz)
//!
//! 1. **Local sampling, no IPC.** Foreground window + idle via
//!    st-tracker-win; the sample feeds [`cycle::ObsAccumulator`], which
//!    collapses ~1 Hz samples into grid-aligned observations (see
//!    [`cycle::COLLAPSE_WINDOW_SECS`] for why the grid is load-bearing).
//! 2. **Connect**, only if the link is down and the backoff allows it.
//! 3. **`Request::ReportUsage`** — one batch of completed observations,
//!    ascending by `observed_at_utc`. Success (`Response::Accepted`) carries
//!    the agent's ingest instant, logged against our send clock as a skew
//!    signal. Errors retain the batch: the contract guarantees all-or-nothing
//!    discard plus idempotency on `(app_key, observed_at_utc)`, so resending
//!    next cycle is safe.
//! 4. **`Request::Status`** — every cycle. Refreshes `pin_configured` (which
//!    gates the overlay's Quit button) and doubles as liveness proof.
//! 5. **`Request::BlockedApps`** — *not* every cycle. Refetched when focus
//!    changed or the cache aged past [`cycle::BLOCKEDAPPS_MAX_AGE_SECS`];
//!    rationale in [`cycle::needs_blocked_refresh`].
//! 6. **Overlay reconcile** — [`cycle::plan_cycle`] turns the refreshed
//!    picture into show/stay/dismiss; showing re-snapshots focus through one
//!    `GetForegroundWindow` handle ([`snapshot`]) so subject and rect agree.
//!
//! # How Quit/PIN gating works
//!
//! The old overlay rendered Quit unconditionally, then sent an empty PIN the
//! agent rejects whenever a PIN is configured — a button that lies. Now
//! `pin_configured` (from step 4) picks the mode: without a PIN the overlay
//! shows Quit/+15 min (empty PIN succeeds); with a PIN it shows the mouse
//! PIN pad + extend path and **no Quit at all** (see [`overlay`]). Unknown
//! status fails safe toward "PIN present".
//!
//! # Loss bounds
//!
//! Batches are tiny (one observation per ~10 s of continuous focus) and
//! buffered only until the next successful report; a hard kill loses at most
//! one collapse window plus the current batch, and the outbox is capped at
//! [`cycle::MAX_OUTBOX_OBSERVATIONS`] so a permanently failing report path
//! cannot grow memory without bound.

mod autostart;
mod backoff;
mod cycle;
mod link;
mod snapshot;

#[cfg(windows)]
mod hud;
#[cfg(windows)]
mod overlay;

/// Non-Windows shim: overlays are Win32 work, and the agent transport does not
/// exist off-Windows yet either. Keeping the type present lets this binary
/// compile cross-platform (the same policy st-tracker-win follows) while the
/// runtime story stays honestly Windows-first.
#[cfg(not(windows))]
mod overlay {
    pub struct OverlayRun;

    impl OverlayRun {
        pub fn dismiss_and_join(self) {}
    }
}
#[cfg(not(windows))]
mod hud {
    pub struct HudOverlayRun;
    impl HudOverlayRun {
        pub fn dismiss(self) {}
        pub fn update(&self, _r: i64, _t: bool) {}
    }
}

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Context;
use chrono::{DateTime, Utc};
use st_core::model::AppKey;
use st_ipc::{transport, ObservationDto, ReportUsageDto, Request, Response, PIPE_NAME};
use tracing_appender::non_blocking::WorkerGuard;
// SubscriberExt/Layer for stacking the two-layer registry in init_tracing.
use tracing_subscriber::prelude::*;

/// Cycle cadence. Matches the ~1 Hz the sampling contract promises.
const POLL: Duration = Duration::from_millis(1000);

/// Exit status used exclusively for "another session helper is already
/// running". Distinct from generic failure so launchers and scripts can
/// recognise the duplicate-startup case without parsing stderr.
const EXIT_ALREADY_RUNNING: i32 = 2;

/// How many daily log files to keep before the oldest is deleted; see the
/// agent's identically named constant for why this is bounded at all.
const MAX_LOG_FILES: usize = 14;

/// First reconnect delay; doubled per consecutive failure up to [`CONNECT_CAP`].
const CONNECT_BASE: Duration = Duration::from_millis(250);
const CONNECT_CAP: Duration = Duration::from_secs(8);

/// Ingest-clock divergence above which the skew log line escalates to a
/// warning (the agent clamps wild stamps itself; this is the visible trail).
const SKEW_WARN_SECS: i64 = 5;

/// Everything the frame loop learns and remembers between cycles.
struct SessionState {
    /// Last known from Status. Starts `true`: until proven otherwise, assume
    /// the safer overlay mode where no Quit button can appear.
    pin_configured: bool,
    blocked: Vec<st_ipc::BlockedAppDto>,
    blocked_fetched_at: Option<Instant>,
    prev_focused: Option<AppKey>,
    /// Tracks ReportUsage health across cycles purely to log *transitions*
    /// once instead of spamming a warning every second while the agent lacks
    /// ingest support.
    pub usage_flowing: Option<bool>,
    pub hud: Option<st_ipc::HudStateDto>,
}

impl SessionState {
    fn cache_valid(&self) -> bool {
        self.blocked_fetched_at
            .is_some_and(|t| t.elapsed() < Duration::from_secs(cycle::BLOCKEDAPPS_MAX_AGE_SECS))
    }
}

fn main() -> anyhow::Result<()> {
    // Autostart CLI dispatch comes before EVERYTHING below — the
    // single-instance guard included. Managing logon autostart must work
    // while (or especially *because*) a helper is already running: `--autostart
    // off` would otherwise be refused by the guard with "already running",
    // and a registry query has no business creating log files either. With no
    // arguments this branch does nothing at all, leaving the startup sequence
    // byte-identical to before.
    let cli_args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(exit_code) = autostart::handle_cli(&cli_args) {
        std::process::exit(exit_code);
    }

    // Single-instance guard BEFORE anything else (tracing included): a second
    // helper would sample the same desktop and double-count every observation,
    // while two overlays would fight over one keyboard hook and one desktop
    // rectangle. The subscriber does not exist yet, so the refusal goes
    // straight to stderr with the *why* spelled out for whoever launched it.
    // The binding lives to the end of `main`: dropping it releases the mutex.
    #[cfg(windows)]
    let _single_instance = {
        let mutex_name = single_instance_mutex_name();
        match st_win32::acquire_single_instance(&mutex_name) {
            Ok(guard) => guard,
            Err(st_win32::AlreadyRunning) => {
                eprintln!(
                    "error: a screentime-session helper is already running in this login \
                     session ({mutex_name} held); starting another would double-count usage \
                     and stack two block overlays. Exiting."
                );
                std::process::exit(EXIT_ALREADY_RUNNING);
            }
        }
    };

    // Hold the returned WorkerGuard for the whole process (bound in `main`);
    // see init_tracing for why letting it die early silently kills file logs.
    let _log_guard = init_tracing(&session_log_dir());

    tracing::info!("screentime-session running (sampling front)");

    #[cfg(windows)]
    // Titles stay off: they leak documents and credentials, the wire field is
    // advisory only, and the agent-side opt-in setting has not reached this
    // binary yet. Cheap to flip once Status carries it.
    let mut tracker = st_tracker_win::Win32WindowTracker::new(false);
    #[cfg(windows)]
    let mut idle_mon = st_tracker_win::Win32IdleMonitor::new(cycle::IDLE_BUCKET_SECS);

    let mut stream: Option<transport::PipeStream> = None;
    let mut backoff = backoff::Backoff::new(CONNECT_BASE, CONNECT_CAP);
    let mut retry_at = Instant::now();
    let mut acc = cycle::ObsAccumulator::default();
    let mut outbox: Vec<ObservationDto> = Vec::new();
    let mut sess = SessionState {
        pin_configured: true,
        blocked: Vec::new(),
        blocked_fetched_at: None,
        prev_focused: None,
        usage_flowing: None,
        hud: None,
    };
    // App id paired with its live overlay thread handle.
    let mut active: Option<(i64, overlay::OverlayRun)> = None;
    let mut active_hud: Option<((i32, i32, i32, i32), hud::HudOverlayRun)> = None;

    loop {
        let tick = Instant::now();
        let now = Utc::now();

        // -- 1. Local sampling ------------------------------------------------
        #[cfg(windows)]
        let sample = take_sample(&mut tracker, &mut idle_mon);
        #[cfg(not(windows))]
        let sample: Option<cycle::SampleOutcome> = None;
        outbox.extend(acc.offer(
            sample.clone().unwrap_or(cycle::SampleOutcome {
                key: None,
                title: None,
                idle_seconds: 0,
            }),
            now,
        ));
        let focused = sample.and_then(|s| s.key);
        let focused_changed = focused != sess.prev_focused;
        if focused_changed && focused.is_some() {
            tracing::debug!(app = ?focused, "focused app changed");
        }

        // Bound buffered work if reporting has been failing for a long time.
        if outbox.len() > cycle::MAX_OUTBOX_OBSERVATIONS {
            let dropped = outbox.len() - cycle::MAX_OUTBOX_OBSERVATIONS;
            outbox.drain(..dropped);
            tracing::warn!(dropped, "outbox overflow; oldest observations discarded");
        }

        // -- 2. Connection ----------------------------------------------------
        if stream.is_none() && Instant::now() >= retry_at {
            match transport::client_connect(PIPE_NAME) {
                Ok(s) => {
                    tracing::info!("connected to agent");
                    stream = Some(s);
                }
                Err(e) => {
                    retry_at = Instant::now() + backoff.on_failure();
                    tracing::warn!(error = %e, "agent unreachable; backing off");
                }
            }
        }

        // -- 3.–5. Frames over the persistent link ----------------------------
        let fetch_blocked = cycle::needs_blocked_refresh(cycle::CycleFacts {
            focused_key: focused.as_ref(),
            focused_key_changed: focused_changed,
            cache_valid: sess.cache_valid(),
            pin_configured: sess.pin_configured,
            overlay_active_app: active.as_ref().map(|(id, _)| *id),
        });
        if let Some(s) = stream.as_mut() {
            // Reset the backoff only after a *healthy exchange*, not merely a
            // TCP-style connect: a link that dies mid-frame must keep backing
            // off, exactly as the module docs promise.
            if run_frames(s, &mut outbox, &mut sess, fetch_blocked, now) {
                backoff.on_success();
            } else {
                stream = None;
            }
        }

        // -- 6. Overlay reconcile ---------------------------------------------
        let plan = cycle::plan_cycle(
            cycle::CycleFacts {
                focused_key: focused.as_ref(),
                focused_key_changed: focused_changed,
                cache_valid: sess.cache_valid(),
                pin_configured: sess.pin_configured,
                overlay_active_app: active.as_ref().map(|(id, _)| *id),
            },
            &sess.blocked,
        );
        match plan.overlay {
            cycle::OverlayCmd::Stay => {}
            cycle::OverlayCmd::Dismiss => {
                if let Some((id, run)) = active.take() {
                    tracing::info!(app = id, "block lifted or focus lost; dismissing");
                    run.dismiss_and_join();
                }
            }
            cycle::OverlayCmd::Show { app_id, gate } => {
                // Tear the old one down (and join it) before spawning, so two
                // keyboard hooks never overlap needlessly.
                if let Some((_, old)) = active.take() {
                    old.dismiss_and_join();
                }
                match show_overlay(app_id, &sess.blocked, gate) {
                    Some(run) => active = Some((app_id, run)),
                    // Focus moved between decision and snapshot; next cycle
                    // re-plans with fresh facts.
                    None => tracing::debug!(app = app_id, "overlay deferred; focus moved"),
                }
            }
        }
        sess.prev_focused = focused;

        // -- 7. HUD reconcile -------------------------------------------------
        #[cfg(windows)]
        {
            if let Some(hud_state) = &sess.hud {
                if let Some(snap) = snapshot::focused_snapshot() {
                    if snap.rect.2 > 0 && snap.rect.3 > 0 {
                        // Check if we need to respawn because window moved or changed
                        
                        
                        let should_respawn = active_hud.as_ref().map(|(rect, _)| *rect != snap.rect).unwrap_or(true);
                        
                        if should_respawn {
                            if let Some((_, run)) = active_hud.take() {
                                run.dismiss();
                            }
                            active_hud = Some((snap.rect, hud::spawn_hud_overlay(snap.rect)));
                        }
                        
                        if let Some((_, ref run)) = active_hud {
                            run.update(hud_state.remaining_secs, hud_state.is_timer);
                        }
                    }
                }
            } else {
                if let Some((_, run)) = active_hud.take() {
                    run.dismiss();
                }
            }
        }

        // Hold the cadence even when a cycle's work took real time.
        let spent = tick.elapsed();
        if spent < POLL {
            std::thread::sleep(POLL - spent);
        }
    }
}

#[cfg(windows)]
/// One local sample through st-tracker-win's public API.
///
/// Idle mapping into the wire field: `Active` reports 0 (input seen within
/// the threshold), `Idle` reports the raw seconds (the monitor computed them
/// anyway), `Locked` saturates ("maximally idle"). A failed idle read assumes
/// presence — crediting uncertain time is the enforcement-safe direction.
fn take_sample(
    tracker: &mut st_tracker_win::Win32WindowTracker,
    idle_mon: &mut st_tracker_win::Win32IdleMonitor,
) -> Option<cycle::SampleOutcome> {
    use st_core::platform::{IdleMonitor, WindowTracker};

    let aw = match tracker.active_window() {
        Ok(Some(aw)) => aw,
        Ok(None) => return None, // shell/lock/desktop: credit nobody
        Err(e) => {
            tracing::debug!(error = %e, "foreground query failed");
            return None;
        }
    };
    let idle_seconds = match idle_mon.idle_state() {
        Ok(st_core::platform::IdleState::Active) => 0,
        Ok(st_core::platform::IdleState::Idle { for_secs }) => {
            u32::try_from(for_secs).unwrap_or(u32::MAX)
        }
        Ok(st_core::platform::IdleState::Locked) => u32::MAX,
        Err(e) => {
            tracing::debug!(error = %e, "idle query failed");
            0
        }
    };
    Some(cycle::SampleOutcome {
        key: Some(aw.key),
        title: aw.title,
        idle_seconds,
    })
}

/// Steps 3–5 against one live connection. Returns `false` when the link must
/// be recycled (any framing anomaly poisons the stream).
fn run_frames(
    stream: &mut transport::PipeStream,
    outbox: &mut Vec<ObservationDto>,
    sess: &mut SessionState,
    fetch_blocked: bool,
    sent_at: DateTime<Utc>,
) -> bool {
    // 3. Usage batch. Ascending order is contractual; sort defensively even
    // though the accumulator produces ascending stamps. Sent every cycle so
    // empty batches (e.g. idle/desktop) keep the agent's tracking liveness fresh.
    outbox.sort_by(|a, b| a.observed_at_utc.cmp(&b.observed_at_utc));
    let report = ReportUsageDto {
        observations: outbox.clone(),
    };
    match link::round_trip(stream, &Request::ReportUsage { report }) {
        Ok(Response::Accepted { effective_utc, hud }) => {
            log_ingest_ack(&effective_utc, sent_at);
            outbox.clear();
            if sess.usage_flowing != Some(true) {
                tracing::info!("usage reporting acknowledged by agent");
            }
            sess.usage_flowing = Some(true);
            sess.hud = hud;
        }
        // Whole-batch discard + idempotency makes retaining correct.
        Ok(Response::Error { code, message }) => {
            if sess.usage_flowing != Some(false) {
                tracing::info!(?code, %message, "agent rejected usage batch; will retry");
            }
            sess.usage_flowing = Some(false);
        }
        Ok(other) => {
            tracing::warn!(?other, "unexpected response to ReportUsage; recycling link");
            return false;
        }
        Err(e) => {
            tracing::debug!(error = %e, "usage report failed; recycling link");
            return false;
        }
    }

    // 4. Status: pin gate input + liveness.
    match link::round_trip(stream, &Request::Status) {
        Ok(Response::Status(dto)) => sess.pin_configured = dto.pin_configured,
        Ok(other) => {
            tracing::warn!(?other, "unexpected response to Status; recycling link");
            return false;
        }
        Err(e) => {
            tracing::debug!(error = %e, "status failed; recycling link");
            return false;
        }
    }

    // 5. Blocked apps, only when the refresh policy asked.
    if fetch_blocked {
        match link::round_trip(stream, &Request::BlockedApps) {
            Ok(Response::BlockedApps(dto)) => {
                sess.blocked = dto.blocked;
                sess.blocked_fetched_at = Some(Instant::now());
            }
            Ok(other) => {
                tracing::warn!(?other, "unexpected response to BlockedApps; recycling link");
                return false;
            }
            Err(e) => {
                tracing::debug!(error = %e, "blocked-apps failed; recycling link");
                return false;
            }
        }
    }
    true
}

/// Compare the agent's ingest instant against our send clock. Small drift is
/// routine (documented clock rules); large drift gets a warning because it
/// foreshadows clamped day-bucketing.
fn log_ingest_ack(effective_utc: &str, sent_at: DateTime<Utc>) {
    match DateTime::parse_from_rfc3339(effective_utc) {
        Ok(t) => {
            let skew = (t.with_timezone(&Utc) - sent_at).num_seconds().abs();
            if skew > SKEW_WARN_SECS {
                tracing::warn!(skew_secs = skew, "helper/agent clocks diverge");
            } else {
                tracing::debug!(skew_secs = skew, "ingest ack");
            }
        }
        Err(_) => tracing::debug!("ingest ack carried unparsable timestamp"),
    }
}

#[cfg(windows)]
/// Spawn the overlay for `app_id`, if the world still agrees.
///
/// The decision came from this cycle's tracker sample; the geometry comes from
/// a fresh single-handle snapshot. Re-matching the snapshot's key against the
/// blocked entry closes the remaining gap between those two instants: if focus
/// moved meanwhile, we defer to the next cycle rather than cover a stranger.
fn show_overlay(
    app_id: i64,
    blocked: &[st_ipc::BlockedAppDto],
    gate: cycle::QuitGate,
) -> Option<overlay::OverlayRun> {
    let entry = blocked.iter().find(|b| b.app_id == app_id)?;
    let snap = snapshot::focused_snapshot()?;
    if !cycle::app_key_matches(&entry.app_key, &snap.key) {
        return None;
    }
    if snap.rect.2 <= 0 || snap.rect.3 <= 0 {
        return None; // degenerate geometry: caller convention says don't bother
    }
    let mode = match gate {
        cycle::QuitGate::ButtonsAvailable => overlay::OverlayMode::Buttons,
        cycle::QuitGate::PinLocked => overlay::OverlayMode::PinExtend,
    };
    let pid = snap.pid;
    let target_hwnd = snap.hwnd;
    tracing::info!(app = app_id, pid, ?mode, "showing block overlay");
    Some(overlay::spawn_overlay(
        snap.rect,
        overlay::OverlayCallbacks::new(
            move || close_app_direct(app_id, pid, target_hwnd),
            move |pin| extend_app(app_id, &pin),
        ),
        mode,
        entry.label.clone(),
    ))
}

/// "Quit" from the overlay: instantly terminate the target app and notify the agent.
fn close_app_direct(app_id: i64, pid: u32, target_hwnd: isize) -> bool {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
        use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

        if target_hwnd != 0 {
            let _ = PostMessageW(
                HWND(target_hwnd as *mut std::ffi::c_void),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            );
        }
        if pid != 0 {
            if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                let _ = TerminateProcess(handle, 1);
                let _ = CloseHandle(handle);
            }
        }
    }

    std::thread::spawn(move || {
        let _ = request_agent(Request::CloseApps {
            app_id,
            pin: String::new(),
        });
    });

    true
}

/// "+15 minutes" from the overlay, carrying whatever the pad collected (empty
/// when no PIN is configured, which the agent accepts in that mode alone).
fn extend_app(app_id: i64, pin: &str) -> bool {
    match request_agent(Request::GrantOverride {
        target: st_ipc::LimitTargetDto::App { id: app_id },
        seconds: 15 * 60,
        pin: pin.to_string(),
    }) {
        Ok(Response::Accepted { .. }) => true,
        Ok(Response::Error {
            code: st_ipc::ErrorCode::BadPin,
            ..
        }) => {
            tracing::warn!("extend refused: wrong PIN");
            false
        }
        Ok(other) => {
            tracing::warn!(?other, "unexpected extend response");
            false
        }
        Err(e) => {
            tracing::warn!(error = %e, "extend request failed");
            false
        }
    }
}

/// One-shot IPC round trip to the agent, used by the overlay callbacks.
fn request_agent(request: Request) -> anyhow::Result<Response> {
    let mut stream = transport::client_connect(PIPE_NAME)?;
    let response = link::round_trip(&mut stream, &request)?;
    Ok(response)
}

// ---------------------------------------------------------------------------
// Startup plumbing: single-instance naming + two-layer logging.
//
// Mirrors the agent's setup on purpose: same EnvFilter semantics, same
// daily-rolling shape, same WorkerGuard lifetime rule. The two binaries stay
// deliberately independent (each owns its wiring), which is why this is a
// small copy rather than shared code — st-win32 is Win32 plumbing, not a
// logging facade.
// ---------------------------------------------------------------------------

/// Kernel mutex name for the helper's single-instance guard.
///
/// `Local\` on purpose — the opposite choice from the agent: the helper is
/// one per LOGIN SESSION, because every desktop must sample its own
/// foreground window, so the guard must NOT span sessions. The username is
/// appended for fast user switching clarity; when it cannot be resolved the
/// bare name still guarantees one-helper-per-session, the invariant that
/// actually matters.
fn single_instance_mutex_name() -> String {
    single_instance_mutex_name_for(std::env::var("USERNAME").ok().as_deref())
}

/// Pure decision core of [`single_instance_mutex_name`].
fn single_instance_mutex_name_for(username: Option<&str>) -> String {
    match username {
        Some(user) if !user.is_empty() => format!(r"Local\screentime-session-{user}"),
        _ => r"Local\screentime-session".to_string(),
    }
}

/// Resolves the helper's log directory from the environment.
///
/// Rules, in priority order:
/// 1. `SCREENTIME_DATA_DIR` set → `<dir>/logs`. Dev parity: a developer
///    pointing the agent at a scratch tree gets the helper's logs in the same
///    place instead of scattered across user profiles.
/// 2. Otherwise `%LOCALAPPDATA%` → `<LOCALAPPDATA>/screentime/logs`. The
///    per-user location is correct in production because the helper runs
///    unprivileged inside a login session; ProgramData would invite
///    cross-user write contention.
/// 3. No usable `LOCALAPPDATA` (stripped-down contexts) → `./screentime-logs`
///    beside the working directory: last resort, still discoverable.
///
/// Resolution never fails outright — an unusable *directory* is handled by
/// init_tracing's console-only fallback instead of refusing to start.
fn session_log_dir() -> PathBuf {
    resolve_log_dir(
        std::env::var("SCREENTIME_DATA_DIR").ok().as_deref(),
        std::env::var("LOCALAPPDATA").ok().as_deref(),
    )
}

/// Pure decision core of [`session_log_dir`], parameterised so tests exercise
/// the rules without touching process-global environment state. Blank values
/// count as unset: an empty override silently resolving to a relative `logs`
/// folder would scatter files unpredictably.
fn resolve_log_dir(data_dir_env: Option<&str>, local_appdata_env: Option<&str>) -> PathBuf {
    let data_dir = data_dir_env.filter(|v| !v.is_empty());
    let local = local_appdata_env.filter(|v| !v.is_empty());
    match data_dir {
        Some(dir) => PathBuf::from(dir).join("logs"),
        None => match local {
            Some(base) => PathBuf::from(base).join("screentime").join("logs"),
            None => PathBuf::from("screentime-logs"),
        },
    }
}

/// Installs the session helper's tracing stack: one filter, two destinations.
///
/// Console output serves interactive development; a daily-rolling file serves
/// incidents — the helper is typically started detached from any console, so
/// stdout diagnostics would otherwise vanish entirely. Both layers share ONE
/// `RUST_LOG`-derived filter placed atop the registry stack so the file sees
/// exactly what the console would.
///
/// # The classic tracing-appender trap (why this returns a guard)
///
/// `non_blocking` hands back a [`WorkerGuard`] owning the background writer
/// thread; dropping it shuts that thread down and can lose buffered lines —
/// most commonly by letting it die inside the init function as a temporary.
/// `main` must therefore hold the return value for the whole process lifetime.
///
/// A broken log sink must never stop sampling: if the log directory cannot be
/// created or opened, this falls back to console-only logging with a loud
/// warning instead of failing startup.
fn init_tracing(log_dir: &Path) -> Option<WorkerGuard> {
    // Built once for both layers; RUST_LOG honoured, "info" when unset —
    // unchanged from the previous console-only setup.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    match build_daily_appender(log_dir, "screentime-session.log") {
        Ok(appender) => {
            // Non-blocking on purpose: a slow disk must never delay the 1 Hz
            // sampling/reporting cycle behind a logging syscall.
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let stdout_layer = tracing_subscriber::fmt::layer();
            let file_layer = tracing_subscriber::fmt::layer()
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
            tracing_subscriber::fmt().with_env_filter(filter).init();
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
        // `screentime-session.log.<yyyy-mm-dd>`.
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
    use std::sync::Mutex;

    #[test]
    fn log_dir_prefers_screentime_data_dir_for_dev_parity() {
        assert_eq!(
            resolve_log_dir(Some(r"C:\dev\data"), Some(r"C:\Users\u\AppData\Local")),
            PathBuf::from(r"C:\dev\data").join("logs")
        );
    }

    #[test]
    fn log_dir_defaults_to_localappdata_screentime_logs() {
        assert_eq!(
            resolve_log_dir(None, Some(r"C:\Users\u\AppData\Local")),
            PathBuf::from(r"C:\Users\u\AppData\Local\screentime\logs")
        );
    }

    #[test]
    fn log_dir_without_any_env_falls_back_to_a_relative_folder() {
        assert_eq!(
            resolve_log_dir(None, None),
            PathBuf::from("screentime-logs")
        );
    }

    #[test]
    fn blank_env_values_are_treated_as_unset_for_log_dir() {
        assert_eq!(
            resolve_log_dir(Some(""), Some("")),
            PathBuf::from("screentime-logs")
        );
        assert_eq!(
            resolve_log_dir(Some(""), Some(r"C:\Users\u\AppData\Local")),
            PathBuf::from(r"C:\Users\u\AppData\Local\screentime\logs")
        );
    }

    #[test]
    fn session_log_dir_reads_the_real_environment() {
        // Serialised against any future env-touching tests: env vars are
        // process-global, and parallel test threads would race. The variable
        // is restored so neighbouring tests observe pristine state.
        static ENV_LOCK: Mutex<()> = Mutex::new(());
        let _env = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        const KEY: &str = "SCREENTIME_DATA_DIR";
        let original = std::env::var(KEY).ok();
        std::env::set_var(KEY, r"target\test-log-dir-scratch");
        let resolved = session_log_dir();
        match original {
            Some(v) => std::env::set_var(KEY, v),
            None => std::env::remove_var(KEY),
        }

        assert_eq!(
            resolved,
            PathBuf::from(r"target\test-log-dir-scratch").join("logs")
        );
    }

    #[test]
    fn guard_mutex_is_per_user_under_the_local_namespace() {
        assert_eq!(
            single_instance_mutex_name_for(Some("alice")),
            r"Local\screentime-session-alice"
        );
    }

    #[test]
    fn guard_mutex_falls_back_to_a_bare_local_name_without_a_username() {
        assert_eq!(
            single_instance_mutex_name_for(None),
            r"Local\screentime-session"
        );
        assert_eq!(
            single_instance_mutex_name_for(Some("")),
            r"Local\screentime-session"
        );
    }
}
