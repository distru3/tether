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
mod fullscreen;
mod hotkey;
mod link;
mod logging;
mod snapshot;
#[allow(unused_imports)]
use fullscreen::*;
#[allow(unused_imports)]
use hotkey::*;
#[allow(unused_imports)]
use logging::*;

#[cfg(windows)]
mod hud;
#[cfg(windows)]
mod mpo;
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
        pub fn maintain(&mut self) {}
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

/// High-responsiveness cycle cadence for overlay reconciliation and focus tracking (~100 ms).
const POLL: Duration = Duration::from_millis(100);

/// Cadence for normal usage reporting batch flush to the agent (~1 Hz).
const REPORT_INTERVAL: Duration = Duration::from_millis(1000);

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
    pub hud_app: Option<AppKey>,
    pub show_hud_overlay: bool,
    pub show_hud_in_fullscreen: bool,
    pub hud_peek_hotkey: String,
    pub alert_volume: i64,
    pub last_remaining: Option<i64>,
    pub peek_until: Option<Instant>,
}

impl SessionState {
    fn cache_valid(&self) -> bool {
        self.blocked_fetched_at
            .is_some_and(|t| t.elapsed() < Duration::from_secs(cycle::BLOCKEDAPPS_MAX_AGE_SECS))
    }
}

/// Opt out of RivaTuner Statistics Server (RTSS / MSI Afterburner) hooking.
///
/// RTSS checks for this export upon DLL injection. Setting this to 0x0 signals
/// to RTSS that this is a 2D utility desktop overlay and must not be hooked or
/// overlaid with OSD statistics.
#[cfg(windows)]
#[no_mangle]
pub static RTSSHooksCompatibility: u32 = 0x00000000;

#[cfg(windows)]
fn ensure_rtss_exclusion() {
    // Best effort only: the RTSS profile folder is under Program Files, which
    // this unprivileged helper normally cannot write. The NSIS installer
    // (`ui/src-tauri/installer_hooks.nsh`) writes the real profiles elevated;
    // this covers elevated development runs.
    // If RTSS is installed, drop an application profile ensuring EnableHooking=0
    let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") else {
        return;
    };
    let rtss_profiles = PathBuf::from(program_files_x86)
        .join("RivaTuner Statistics Server")
        .join("Profiles");
    if rtss_profiles.is_dir() {
        let profile_cfg = rtss_profiles.join("screentime-session.exe.cfg");
        if !profile_cfg.exists() {
            let _ = std::fs::write(&profile_cfg, "[Hooking]\nEnableHooking = 0\n");
        }
    }
}

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        // Reference symbol to ensure it isn't stripped by dead code analysis
        let _ = &RTSSHooksCompatibility;
        ensure_rtss_exclusion();
    }
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
    // while two would fight over the HUD and keep re-showing the block
    // overlay for each other. The subscriber does not exist yet, so the refusal goes
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
        hud_app: None,
        show_hud_overlay: true,
        show_hud_in_fullscreen: false,
        hud_peek_hotkey: "Ctrl+Alt+T".to_string(),
        alert_volume: 80,
        last_remaining: None,
        peek_until: None,
    };
    #[cfg(windows)]
    let hotkey_mgr = HotkeyManager::spawn(sess.hud_peek_hotkey.clone());
    // App id paired with its live overlay thread handle.
    let mut active: Option<(i64, st_core::model::AppKey, overlay::OverlayRun)> = None;
    let mut active_hud: Option<(isize, hud::HudOverlayRun)> = None;
    let mut next_1hz_report = Instant::now();
    let mut prev_raw_focused: Option<st_core::model::AppKey> = None;

    // Proactive app discovery runs once per process lifetime, on first successful
    // agent connection. It must run in the session helper (not the agent) because
    // the agent runs as SYSTEM in Session 0 and cannot read HKCU / %APPDATA%.
    let mut discovery_done = false;

    loop {
        let tick = Instant::now();
        let now = Utc::now();
        let is_1hz_tick = tick >= next_1hz_report;

        // -- 1. Local sampling ------------------------------------------------
        #[cfg(windows)]
        let sample = take_sample(&mut tracker, &mut idle_mon);
        #[cfg(not(windows))]
        let sample: Option<cycle::SampleOutcome> = None;

        let raw_focused = sample.as_ref().and_then(|s| s.key.clone());
        let raw_focused_changed = raw_focused != prev_raw_focused;
        if raw_focused_changed && raw_focused.is_some() {
            tracing::debug!(app = ?raw_focused, "focused app changed");
        }
        prev_raw_focused = raw_focused.clone();

        // -- 2. Connection ----------------------------------------------------
        if stream.is_none() && Instant::now() >= retry_at {
            match transport::client_connect(PIPE_NAME) {
                Ok(s) => {
                    tracing::info!("connected to agent");
                    stream = Some(s);

                    // -- 2a. Proactive discovery (once, on first connect) ------
                    if !discovery_done {
                        discovery_done = true;
                        #[cfg(windows)]
                        std::thread::Builder::new()
                            .name("discovery".into())
                            .spawn(|| {
                                tracing::debug!("starting proactive app discovery scan");
                                let apps = st_tracker_win::scan_installed_apps();
                                if apps.is_empty() {
                                    tracing::debug!("discovery found no apps");
                                    return;
                                }
                                tracing::debug!(count = apps.len(), "discovery scan complete; registering with agent");
                                match transport::client_connect(PIPE_NAME) {
                                    Ok(mut s) => {
                                        let req = Request::RegisterDiscoveredApps { apps };
                                        if let Err(e) = st_ipc::write_message(&mut s, &req) {
                                            tracing::warn!(error = %e, "failed to send discovered apps to agent");
                                            return;
                                        }
                                        match st_ipc::read_message::<_, Response>(&mut s) {
                                            Ok(Response::Accepted { .. }) => {
                                                tracing::debug!("agent accepted discovered apps");
                                            }
                                            Ok(other) => {
                                                tracing::warn!(response = ?other, "unexpected response from agent on discovered apps");
                                            }
                                            Err(e) => {
                                                tracing::warn!(error = %e, "error reading response for discovered apps");
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "could not connect to agent for discovery registration");
                                    }
                                }
                            })
                            .ok();
                    }
                }
                Err(e) => {
                    retry_at = Instant::now() + backoff.on_failure();
                    tracing::warn!(error = %e, "agent unreachable; backing off");
                }
            }
        }

        let is_ui_focused = raw_focused
            .as_ref()
            .map(|k| {
                let b = k.basename();
                b.eq_ignore_ascii_case("screentime-ui.exe")
                    || b.eq_ignore_ascii_case("screentime-ui")
            })
            .unwrap_or(false);

        let is_overlay_focused = is_ui_focused && is_overlay_window_focused();

        // If the focused window belongs specifically to our secondary "Tether Overlay"
        // and an overlay is active, map it to the active blocked app's key so the cycle
        // knows the user is interacting with the block screen rather than dismissing it.
        // If the user focused the main "Tether" window, is_overlay_focused is false,
        // which correctly lets the cycle dismiss the overlay card.
        let effective_focused = match (&active, is_overlay_focused) {
            (Some((_, active_key, _)), true) => Some(active_key.clone()),
            _ => raw_focused.clone(),
        };
        let effective_focused_changed = effective_focused != sess.prev_focused;

        // Record observations into the accumulator on focus transitions or ~1 Hz boundary
        if is_1hz_tick || effective_focused_changed {
            let mut eff_sample = sample.clone().unwrap_or(cycle::SampleOutcome {
                key: None,
                title: None,
                idle_seconds: 0,
            });
            eff_sample.key = effective_focused.clone();
            outbox.extend(acc.offer(eff_sample, now));
        }

        // Bound buffered work if reporting has been failing for a long time.
        if outbox.len() > cycle::MAX_OUTBOX_OBSERVATIONS {
            let dropped = outbox.len() - cycle::MAX_OUTBOX_OBSERVATIONS;
            outbox.drain(..dropped);
            tracing::warn!(dropped, "outbox overflow; oldest observations discarded");
        }

        // -- 3.–5. Frames over the persistent link ----------------------------
        let fetch_blocked = cycle::needs_blocked_refresh(cycle::CycleFacts {
            focused_key: effective_focused.as_ref(),
            focused_key_changed: effective_focused_changed,
            cache_valid: sess.cache_valid(),
            pin_configured: sess.pin_configured,
            overlay_active_app: active.as_ref().map(|(id, _, _)| *id),
        });

        let should_run_frames = is_1hz_tick || effective_focused_changed;

        if should_run_frames {
            if let Some(s) = stream.as_mut() {
                // Reset the backoff only after a *healthy exchange*, not merely a
                // TCP-style connect: a link that dies mid-frame must keep backing
                // off, exactly as the module docs promise.
                if run_frames(
                    s,
                    &mut outbox,
                    &mut sess,
                    fetch_blocked,
                    now,
                    effective_focused.as_ref(),
                    #[cfg(windows)]
                    &hotkey_mgr,
                ) {
                    backoff.on_success();
                } else {
                    stream = None;
                }
            }
            if is_1hz_tick {
                next_1hz_report = tick + REPORT_INTERVAL;
            }
        }

        // -- 6. Overlay reconcile ---------------------------------------------
        let plan = cycle::plan_cycle(
            cycle::CycleFacts {
                focused_key: effective_focused.as_ref(),
                focused_key_changed: effective_focused_changed,
                cache_valid: sess.cache_valid(),
                pin_configured: sess.pin_configured,
                overlay_active_app: active.as_ref().map(|(id, _, _)| *id),
            },
            &sess.blocked,
        );
        match plan.overlay {
            cycle::OverlayCmd::Stay => {
                // Keep re-asserting the block screen while it should be up;
                // see `overlay::OverlayRun::maintain`.
                if let Some((_, _, run)) = active.as_mut() {
                    run.maintain();
                }
            }
            cycle::OverlayCmd::Dismiss => {
                if let Some((id, _, run)) = active.take() {
                    tracing::info!(app = id, "block lifted or focus lost; dismissing");
                    run.dismiss_and_join();
                }
            }
            cycle::OverlayCmd::Show { app_id, gate } => {
                // A different app is now blocked: hide the old block screen
                // before showing the new one.
                if let Some((_, _, old)) = active.take() {
                    old.dismiss_and_join();
                }
                match show_overlay(
                    app_id,
                    &sess.blocked,
                    gate,
                    sess.alert_volume.clamp(0, 100) as u32,
                ) {
                    Some((key, run)) => active = Some((app_id, key, run)),
                    // Focus moved between decision and snapshot; next cycle
                    // re-plans with fresh facts.
                    None => tracing::debug!(app = app_id, "overlay deferred; focus moved"),
                }
            }
        }
        // -- 7. HUD reconcile -------------------------------------------------
        #[cfg(windows)]
        {
            if effective_focused_changed {
                sess.last_remaining = None;
            }

            let current_key = effective_focused.as_ref();
            let hud_matches_focus = sess.hud_app.as_ref() == current_key;

            if active.is_some() || current_key.is_none() || !hud_matches_focus {
                if let Some((_, run)) = active_hud.take() {
                    run.dismiss();
                }
            } else if let Some(hud_state) = &sess.hud {
                if let Some(snap) = snapshot::focused_snapshot() {
                    // Check for milestone transitions (15m, 10m, 5m, 1m)
                    const MILESTONES: &[i64] = &[15 * 60, 10 * 60, 5 * 60, 60];
                    if let Some(prev) = sess.last_remaining {
                        for &m in MILESTONES {
                            if prev > m && hud_state.remaining_secs <= m {
                                play_alert_sound(sess.alert_volume.clamp(0, 100) as u32);
                                sess.peek_until = Some(Instant::now() + Duration::from_secs(4));
                                break;
                            }
                        }
                    }
                    sess.last_remaining = Some(hud_state.remaining_secs);

                    let in_game = is_game_or_fullscreen(&snap);
                    let allow_continuous =
                        sess.show_hud_overlay && (sess.show_hud_in_fullscreen || !in_game);
                    let peek_active = hotkey_mgr.was_pressed_recently(4000)
                        || sess.peek_until.map(|t| Instant::now() < t).unwrap_or(false);

                    if !allow_continuous && !peek_active {
                        // Suppress HUD overlay when continuous HUD is disabled or over full-screen games,
                        // unless temporarily revealed via peek shortcut or milestone alert.
                        #[cfg(windows)]
                        {
                            if let Some((_, ref run)) = active_hud {
                                if !run.is_exiting() {
                                    run.start_exit();
                                }
                            }
                            if let Some((_, ref run)) = active_hud {
                                if !run.is_alive() {
                                    if let Some((_, run)) = active_hud.take() {
                                        run.dismiss();
                                    }
                                }
                            }
                        }
                        #[cfg(not(windows))]
                        if let Some((_, run)) = active_hud.take() {
                            run.dismiss();
                        }
                    } else if snap.rect.2 > 0 && snap.rect.3 > 0 && current_key == Some(&snap.key) {
                        #[cfg(windows)]
                        let should_respawn = active_hud
                            .as_ref()
                            .map(|(hwnd, run)| *hwnd != snap.hwnd || !run.is_alive())
                            .unwrap_or(true);
                        #[cfg(not(windows))]
                        let should_respawn = active_hud
                            .as_ref()
                            .map(|(hwnd, _)| *hwnd != snap.hwnd)
                            .unwrap_or(true);

                        if should_respawn {
                            if let Some((_, run)) = active_hud.take() {
                                run.dismiss();
                            }
                            active_hud = hud::spawn_hud_overlay(snap.hwnd, snap.rect)
                                .map(|run| (snap.hwnd, run));
                        } else if let Some((_, ref run)) = active_hud {
                            #[cfg(windows)]
                            if run.is_exiting() {
                                // Hotkey pressed during exit animation: smoothly reverse back to entrance!
                                run.reverse_to_enter();
                            }
                        }

                        if let Some((_, ref run)) = active_hud {
                            run.update(hud_state.remaining_secs, hud_state.is_timer);
                        }
                    } else if let Some((_, run)) = active_hud.take() {
                        run.dismiss();
                    }
                } else if let Some((_, run)) = active_hud.take() {
                    run.dismiss();
                }
            } else if let Some((_, run)) = active_hud.take() {
                run.dismiss();
            }
        }
        sess.prev_focused = effective_focused;

        // Hold the cadence even when a cycle's work took real time.
        let spent = tick.elapsed();
        let mut sleep_dur = POLL.saturating_sub(spent);
        #[cfg(windows)]
        {
            if let Some((_, ref run)) = active_hud {
                if run.is_exiting() {
                    // Poll at high frequency (50ms) during exit animation to catch hotkey interruption immediately
                    sleep_dur = sleep_dur.min(Duration::from_millis(50));
                } else if !sess.show_hud_overlay {
                    if let Some(rem_ms) = hotkey_mgr.peek_remaining_ms(4000) {
                        if rem_ms > 0 {
                            sleep_dur = sleep_dur.min(Duration::from_millis(rem_ms.max(20)));
                        }
                    }
                }
            }
        }
        if !sleep_dur.is_zero() {
            std::thread::sleep(sleep_dur);
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
    current_focused: Option<&AppKey>,
    #[cfg(windows)] hotkey_mgr: &HotkeyManager,
) -> bool {
    // 3. Usage batch. Ascending order is contractual; sort defensively even
    // though the accumulator produces ascending stamps. Sent every cycle so
    // empty batches (e.g. idle/desktop) keep the agent's tracking liveness fresh.
    outbox.sort_by(|a, b| a.observed_at_utc.cmp(&b.observed_at_utc));
    let report = ReportUsageDto {
        observations: outbox.clone(),
        focused_key: current_focused.cloned(),
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
            sess.hud_app = if sess.hud.is_some() {
                current_focused.cloned()
            } else {
                None
            };
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
        Ok(Response::Status(dto)) => {
            sess.pin_configured = dto.pin_configured;
            sess.show_hud_overlay = dto.show_hud_overlay;
            sess.show_hud_in_fullscreen = dto.show_hud_in_fullscreen;
            sess.alert_volume = dto.alert_volume;
            #[cfg(windows)]
            if sess.hud_peek_hotkey != dto.hud_peek_hotkey {
                sess.hud_peek_hotkey = dto.hud_peek_hotkey.clone();
                hotkey_mgr.update_hotkey(dto.hud_peek_hotkey);
            }
        }
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

/// Plays a modern, smooth, non-intrusive alert chime for overlay milestone alerts and block events.
#[cfg(windows)]
pub fn play_alert_sound(volume_pct: u32) {
    const CHIME_WAV: &[u8] = include_bytes!("assets/chime.wav");
    st_win32::audio::play_wav_scaled(CHIME_WAV, volume_pct);
}

#[cfg(not(windows))]
pub fn play_alert_sound(_volume_pct: u32) {}

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
    alert_volume: u32,
) -> Option<(st_core::model::AppKey, overlay::OverlayRun)> {
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
    let key = snap.key.clone();
    tracing::info!(app = app_id, pid, ?mode, "showing block overlay");
    play_alert_sound(alert_volume);
    let run = overlay::spawn_overlay(
        app_id,
        pid,
        target_hwnd,
        snap.rect,
        overlay::OverlayCallbacks::new(
            move || close_app_direct(app_id, pid, target_hwnd),
            move |pin| extend_app(app_id, &pin),
        ),
        mode,
        entry.label.clone(),
    );
    Some((key, run))
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

    #[test]
    fn game_detection_matches_known_game_binaries_and_patterns() {
        assert!(st_core::games::is_game_executable("cs2.exe"));
        assert!(st_core::games::is_game_executable("CS2.EXE"));
        assert!(st_core::games::is_game_executable(
            "Valorant-Win64-Shipping.exe"
        ));
        assert!(st_core::games::is_game_executable("Cyberpunk2077.exe"));
        assert!(st_core::games::is_game_executable(
            "FortniteClient-Win64-Shipping.exe"
        ));
        assert!(st_core::games::is_game_executable("some_indie_game.exe"));
        assert!(st_core::games::is_game_executable(
            "Project-Win64-Shipping.exe"
        ));
        assert!(!st_core::games::is_game_executable("notepad.exe"));
        assert!(!st_core::games::is_game_executable("chrome.exe"));
        assert!(!st_core::games::is_game_executable("code.exe"));
    }

    #[test]
    fn game_detection_matches_launcher_and_library_paths() {
        assert!(st_core::games::is_game_path(
            r"C:\Program Files (x86)\Steam\steamapps\common\Counter-Strike Global Offensive\game\bin\win64\cs2.exe"
        ));
        assert!(st_core::games::is_game_path(
            r"D:\Epic Games\Fortnite\FortniteGame\Binaries\Win64\FortniteClient-Win64-Shipping.exe"
        ));
        assert!(st_core::games::is_game_path(
            r"C:\Riot Games\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe"
        ));
        assert!(st_core::games::is_game_path(
            r"C:\XboxGames\Halo Infinite\Content\halo-infinite.exe"
        ));
        assert!(!st_core::games::is_game_path(
            r"C:\Program Files\Google\Chrome\Application\chrome.exe"
        ));
        assert!(!st_core::games::is_game_path(
            r"C:\Windows\System32\notepad.exe"
        ));
    }

    #[test]
    #[cfg(windows)]
    fn parse_hotkey_recognizes_standard_and_custom_combinations() {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
        };

        let (mods, vk) = parse_hotkey("Ctrl+Alt+T").expect("valid");
        assert_eq!(mods, MOD_NOREPEAT | MOD_CONTROL | MOD_ALT);
        assert_eq!(vk, 0x54);

        let (mods, vk) = parse_hotkey("Alt+\\").expect("valid");
        assert_eq!(mods, MOD_NOREPEAT | MOD_ALT);
        assert_eq!(vk, 0xDC);

        let (mods, vk) = parse_hotkey("Ctrl+Shift+F9").expect("valid");
        assert_eq!(mods, MOD_NOREPEAT | MOD_CONTROL | MOD_SHIFT);
        assert_eq!(vk, 0x78);

        assert!(parse_hotkey("invalid_combo_with_no_key").is_none());
    }

    #[test]
    fn alert_chime_asset_is_valid_wav() {
        const CHIME_WAV: &[u8] = include_bytes!("assets/chime.wav");
        assert!(CHIME_WAV.len() > 1000);
        assert_eq!(&CHIME_WAV[0..4], b"RIFF");
        assert_eq!(&CHIME_WAV[8..12], b"WAVE");
        // Ensure play_alert_sound runs without panicking with scaling and mute
        play_alert_sound(80);
        play_alert_sound(0);
        play_alert_sound(100);
    }

    #[test]
    #[cfg(windows)]
    fn window_rect_fullscreen_distinguishes_maximized_from_fullscreen() {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::UI::WindowsAndMessaging::{WS_CAPTION, WS_MAXIMIZE, WS_POPUP};

        let mon = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let work = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        }; // 40px taskbar at bottom

        // Standard maximized desktop application (e.g. Chrome, VS Code)
        let max_rect = RECT {
            left: -8,
            top: -8,
            right: 1928,
            bottom: 1048,
        };
        let standard_max_style = WS_MAXIMIZE.0 | WS_CAPTION.0 | 0x00040000; // WS_THICKFRAME
        assert!(!is_window_rect_fullscreen(
            max_rect,
            mon,
            work,
            standard_max_style
        ));

        // Floating/restored window
        let floating_rect = RECT {
            left: 200,
            top: 150,
            right: 1400,
            bottom: 900,
        };
        assert!(!is_window_rect_fullscreen(
            floating_rect,
            mon,
            work,
            WS_CAPTION.0
        ));

        // True borderless fullscreen game (covers taskbar, lacks WS_CAPTION)
        let game_rect = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let borderless_game_style = WS_POPUP.0;
        assert!(is_window_rect_fullscreen(
            game_rect,
            mon,
            work,
            borderless_game_style
        ));

        // Chrome F11 fullscreen (covers taskbar, WS_CAPTION removed)
        assert!(is_window_rect_fullscreen(game_rect, mon, work, 0));

        // Auto-hidden taskbar case: work == mon
        // Maximized app with caption should STILL NOT be considered a fullscreen game
        let auto_hide_max_rect = RECT {
            left: -8,
            top: -8,
            right: 1928,
            bottom: 1088,
        };
        assert!(!is_window_rect_fullscreen(
            auto_hide_max_rect,
            mon,
            mon,
            standard_max_style
        ));

        // But borderless game on auto-hide monitor IS fullscreen
        assert!(is_window_rect_fullscreen(
            game_rect,
            mon,
            mon,
            borderless_game_style
        ));
    }
}
