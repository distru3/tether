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

mod backoff;
mod cycle;
mod link;
mod snapshot;

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

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use st_core::model::AppKey;
use st_ipc::{transport, ObservationDto, ReportUsageDto, Request, Response, PIPE_NAME};

/// Cycle cadence. Matches the ~1 Hz the sampling contract promises.
const POLL: Duration = Duration::from_millis(1000);

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
    usage_flowing: Option<bool>,
}

impl SessionState {
    fn cache_valid(&self) -> bool {
        self.blocked_fetched_at
            .is_some_and(|t| t.elapsed() < Duration::from_secs(cycle::BLOCKEDAPPS_MAX_AGE_SECS))
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

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
    };
    // App id paired with its live overlay thread handle.
    let mut active: Option<(i64, overlay::OverlayRun)> = None;

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
    // though the accumulator produces ascending stamps.
    if !outbox.is_empty() {
        outbox.sort_by(|a, b| a.observed_at_utc.cmp(&b.observed_at_utc));
        let report = ReportUsageDto {
            observations: outbox.clone(),
        };
        match link::round_trip(stream, &Request::ReportUsage { report }) {
            Ok(Response::Accepted { effective_utc }) => {
                log_ingest_ack(&effective_utc, sent_at);
                outbox.clear();
                if sess.usage_flowing != Some(true) {
                    tracing::info!("usage reporting acknowledged by agent");
                }
                sess.usage_flowing = Some(true);
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
    tracing::info!(app = app_id, ?mode, "showing block overlay");
    Some(overlay::spawn_overlay(
        snap.rect,
        overlay::OverlayCallbacks::new(
            move || close_app(app_id),
            move |pin| extend_app(app_id, &pin),
        ),
        mode,
        entry.label.clone(),
    ))
}

/// "Quit" from the overlay: terminate the app's process tree.
///
/// Click actions deliberately ride their own short-lived connection instead of
/// the persistent link: they are rare, latency-tolerant, must work even while
/// the sampler is mid-reconnect, and sharing the link with overlay threads
/// would buy nothing but a mutex.
fn close_app(app_id: i64) -> bool {
    match request_agent(Request::CloseApps {
        app_id,
        pin: String::new(),
    }) {
        Ok(Response::Accepted { .. }) => true,
        Ok(Response::Error {
            code: st_ipc::ErrorCode::BadPin,
            ..
        }) => {
            // Belt and braces: the button is hidden whenever Status said a PIN
            // exists, so landing here means config flipped under us.
            tracing::warn!("quit refused (PIN now required); use Screentime");
            false
        }
        Ok(other) => {
            tracing::warn!(?other, "unexpected close response");
            false
        }
        Err(e) => {
            tracing::warn!(error = %e, "close request failed");
            false
        }
    }
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
