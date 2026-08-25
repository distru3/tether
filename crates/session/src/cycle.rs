//! The pure cycle layer: everything the session loop decides, decided here
//! without touching a pipe or a Win32 handle.
//!
//! # Why this split exists
//!
//! `main.rs` is I/O glue: it samples, moves frames over the pipe and spawns
//! overlay threads. Every *decision* it makes — what to report, when to
//! refresh the blocked picture, whether to show/keep/dismiss an overlay, how a
//! sample collapses into an observation — lives in this module as plain
//! functions over plain data, so all of it runs under `cargo test` without a
//! desktop session or an agent.

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use st_core::model::AppKey;
use st_ipc::{BlockedAppDto, ObservationDto};

/// Width of one collapse window on the Unix clock, in seconds.
///
/// Why grid-aligned fixed windows instead of "close after N samples": the wire
/// contract is idempotent on `(app_key, observed_at_utc)`, which only works if
/// the same stretch of usage always produces the *same* end stamp. Wall-clock
/// alignment gives that for free (a resend of window 17:00:20–17:00:30 carries
/// `17:00:30` again no matter when the helper restarted), and it lets the agent
/// infer each observation's duration as the gap to the next stamp.
pub const COLLAPSE_WINDOW_SECS: i64 = 10;

/// Idle seconds at or beyond which a sample counts as "away" rather than
/// "at desk". The two buckets are what force a collapse-window flush when the
/// user walks away mid-window, so the agent can discount idle-but-focused
/// time per window rather than per sample.
pub const IDLE_BUCKET_SECS: u64 = 60;

/// Maximum age of the cached `BlockedApps` answer before it must be refetched.
/// Bounds how late an app that becomes blocked mid-focus (limit crossed while
/// the user sits in the app) gets its overlay: at most this many seconds,
/// independent of focus changes.
pub const BLOCKEDAPPS_MAX_AGE_SECS: u64 = 15;

/// Ceiling on buffered observations. Normally unreachable: windows close every
/// [`COLLAPSE_WINDOW_SECS`] and are drained each ~1 s cycle. The cap exists so
/// a long-lived `ReportUsage` failure (e.g. an agent too old to implement the
/// ingest) degrades to dropping old data instead of growing memory forever.
pub const MAX_OUTBOX_OBSERVATIONS: usize = 4096;

/// One foreground sample, already reduced to what reporting needs.
///
/// `key: None` means "nothing creditable has focus" (desktop, shell surfaces,
/// lock screen) — normal, not an error. Produced by st-tracker-win's public
/// API in `main`; consumed here as inert data so tests need no Windows.
#[derive(Debug, Clone)]
pub struct SampleOutcome {
    pub key: Option<AppKey>,
    pub title: Option<String>,
    pub idle_seconds: u32,
}

/// Whether the user looked present during a sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    AtDesk,
    Away,
}

/// RFC 3339 UTC with second precision and a literal `Z`, matching the format
/// the IPC contract's examples pin down.
pub fn rfc3339(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn presence_bucket(idle_seconds: u32) -> Presence {
    if idle_seconds >= u32::try_from(IDLE_BUCKET_SECS).unwrap_or(u32::MAX) {
        Presence::Away
    } else {
        Presence::AtDesk
    }
}

/// End instant of the collapse window that contains `now`: the next multiple
/// of [`COLLAPSE_WINDOW_SECS`] on the Unix clock.
fn grid_end(now: DateTime<Utc>) -> DateTime<Utc> {
    let secs = now.timestamp();
    let next = (secs.div_euclid(COLLAPSE_WINDOW_SECS) + 1) * COLLAPSE_WINDOW_SECS;
    Utc.timestamp_opt(next, 0).single().unwrap_or(now)
}

fn fresh_title(title: &Option<String>) -> Option<String> {
    title.as_ref().filter(|t| !t.is_empty()).cloned()
}

/// Collapse buffer turning ~1 Hz samples into contract-shaped observations.
///
/// Consecutive samples of the same `(app_key, presence)` merge into one open
/// window; the window closes (flushing one [`ObservationDto`]) when the app
/// changes, the presence bucket flips, the clock crosses the window edge, or
/// the process drains explicitly. `window_title` is captured from the first
/// titled sample of a window only — sampling it once per collapse window is
/// exactly what the contract promises the agent.
#[derive(Debug, Default)]
pub struct ObsAccumulator {
    pending: Option<PendingWindow>,
}

#[derive(Debug, Clone)]
struct PendingWindow {
    key: AppKey,
    presence: Presence,
    title: Option<String>,
    idle_seconds: u32,
    ends_at: DateTime<Utc>,
}

impl ObsAccumulator {
    /// Feed one sample; returns observations that closed as a result.
    ///
    /// A sample with `key: None` closes any open window (desktop time credits
    /// nobody) and opens nothing.
    pub fn offer(&mut self, sample: SampleOutcome, now: DateTime<Utc>) -> Vec<ObservationDto> {
        let Some(key) = sample.key else {
            return self.close_pending();
        };
        let presence = presence_bucket(sample.idle_seconds);
        let title = fresh_title(&sample.title);

        let same_window = matches!(
            self.pending.as_ref(),
            Some(p) if p.key == key && p.presence == presence
        );
        if !same_window {
            let out = self.close_pending();
            self.pending = Some(PendingWindow {
                key,
                presence,
                title,
                idle_seconds: sample.idle_seconds,
                ends_at: grid_end(now),
            });
            return out;
        }

        let pending = self.pending.as_mut().expect("same_window implies pending");
        pending.idle_seconds = sample.idle_seconds;
        if pending.title.is_none() {
            pending.title = title;
        }
        if now >= pending.ends_at {
            // Window edge crossed: emit it and start the next aligned window.
            let out = self.close_pending();
            self.pending = Some(PendingWindow {
                key,
                presence,
                title: fresh_title(&sample.title),
                idle_seconds: sample.idle_seconds,
                ends_at: grid_end(now),
            });
            return out;
        }
        Vec::new()
    }

    /// Drain immediately: the documented shutdown path ("once before the
    /// helper exits"). The run loop itself is killed externally (console
    /// close / service stop), so production code does not reach this today;
    /// it exists so the drain behaviour is real, tested code rather than
    /// prose, and ready for whoever adds a graceful-stop signal.
    #[allow(dead_code)]
    pub fn flush(&mut self, now: DateTime<Utc>) -> Vec<ObservationDto> {
        let mut out = self.close_pending();
        for obs in &mut out {
            obs.observed_at_utc = rfc3339(now);
        }
        out
    }

    fn close_pending(&mut self) -> Vec<ObservationDto> {
        match self.pending.take() {
            None => Vec::new(),
            Some(p) => vec![ObservationDto {
                app_key: p.key,
                window_title: p.title,
                idle_seconds: p.idle_seconds,
                observed_at_utc: rfc3339(p.ends_at),
            }],
        }
    }
}

/// What the cycle knows when planning, all cheap snapshots taken by `main`.
#[derive(Debug, Clone, Copy)]
pub struct CycleFacts<'a> {
    /// Focused subject from this cycle's sample, if any.
    pub focused_key: Option<&'a AppKey>,
    /// Focused key differs from the previous cycle's (including None↔Some).
    pub focused_key_changed: bool,
    /// Blocked cache exists AND is younger than [`BLOCKEDAPPS_MAX_AGE_SECS`].
    pub cache_valid: bool,
    /// Last known `pin_configured` from the agent's Status.
    pub pin_configured: bool,
    /// App id covered by the currently-live overlay, if any.
    pub overlay_active_app: Option<i64>,
}

/// Whether the session loop should fetch `BlockedApps` this cycle.
///
/// Refresh policy, documented here because it is a deliberate trade:
///
/// * **Focus changed** → refetch. Focus transitions are rare relative to 1 Hz,
///   and the user landing on some app is the moment overlay correctness
///   matters most; paying one frame there buys immediate decisions.
/// * **Cache older than max age** → refetch, so a block that appears with *no*
///   focus change (user crosses their limit mid-session) still surfaces within
///   a bounded, generous window.
/// * Otherwise reuse the cache — polling the full list every second would buy
///   nothing the two rules above miss.
pub fn needs_blocked_refresh(facts: CycleFacts<'_>) -> bool {
    facts.focused_key_changed || !facts.cache_valid
}

/// Overlay command for this cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayCmd {
    /// Current overlay (or absence of one) is correct.
    Stay,
    /// Show (or switch to) the overlay for this app id. Carries the Quit gate
    /// decided for it: the mode is fixed for an overlay's lifetime and only
    /// re-decided on respawn.
    Show { app_id: i64, gate: QuitGate },
    /// No blocked app matches focus; tear any overlay down.
    Dismiss,
}

/// Everything the current cycle should do, decided in one pure step.
pub fn plan_cycle(facts: CycleFacts<'_>, blocked: &[BlockedAppDto]) -> Plan {
    let focused_block = facts.focused_key.and_then(|key| {
        blocked
            .iter()
            .find(|b| app_key_matches(&b.app_key, key))
            .map(|b| b.app_id)
    });

    let overlay = match (facts.overlay_active_app, focused_block) {
        (Some(cur), Some(want)) if cur == want => OverlayCmd::Stay,
        // Covers both cold-show and switching between two blocked apps; the
        // gate is evaluated fresh so a PIN added/removed since the last spawn
        // takes effect at the next transition.
        (_, Some(want)) => OverlayCmd::Show {
            app_id: want,
            gate: quit_gate(facts.pin_configured),
        },
        (Some(_), None) => OverlayCmd::Dismiss,
        (None, None) => OverlayCmd::Stay,
    };

    Plan {
        fetch_blocked: needs_blocked_refresh(facts),
        overlay,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub fetch_blocked: bool,
    pub overlay: OverlayCmd,
}

/// Whether the overlay may offer Quit.
///
/// Audit fix ("Quit button lies"): Quit sends an empty PIN, which the agent
/// accepts **only** when no PIN is configured. So the button may be rendered
/// only under the same condition. When a PIN exists the overlay shows the PIN
/// pad + extend path instead; there is deliberately no Quit affordance behind
/// a PIN — the parent who holds the PIN extends or manages from Screentime.
///
/// Unknown status defaults to [`QuitGate::PinLocked`]: until a Status reply
/// proves otherwise, assume the safer mode where no lying button can appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitGate {
    /// No PIN required: Quit works with an empty PIN and may be shown.
    ButtonsAvailable,
    /// PIN configured: hide Quit, show the PIN pad + extend path.
    PinLocked,
}

pub fn quit_gate(pin_configured: bool) -> QuitGate {
    if pin_configured {
        QuitGate::PinLocked
    } else {
        QuitGate::ButtonsAvailable
    }
}

/// Match a stored `kind:value` app key against a focused app key. Compares the
/// canonical forms; falls back to comparing basenames so a path difference
/// (e.g. the focused path resolving differently than the stored one) does not
/// silently defeat the overlay.
pub fn app_key_matches(stored: &str, focused: &AppKey) -> bool {
    if let Some(stored_key) = AppKey::parse_db_string(stored) {
        if stored_key == *focused {
            return true;
        }
    }
    // Fallback: basename comparison.
    let stored_basename = stored
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(stored)
        .trim_start_matches("win-exe:");
    stored_basename.eq_ignore_ascii_case(focused.basename())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe(path: &str) -> AppKey {
        AppKey::windows_exe(path)
    }

    fn sample(key: Option<AppKey>) -> SampleOutcome {
        SampleOutcome {
            key,
            title: None,
            idle_seconds: 0,
        }
    }

    fn at(unix_secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(unix_secs, 0).single().expect("valid ts")
    }

    #[test]
    fn exact_key_matches() {
        let focused = AppKey::windows_exe("C:\\Games\\Elden Ring\\game\\eldenring.exe");
        assert!(app_key_matches(
            "win-exe:c:\\games\\elden ring\\game\\eldenring.exe",
            &focused
        ));
    }

    #[test]
    fn basename_fallback_matches() {
        let focused = AppKey::windows_exe("C:\\somewhere\\ELDENRING.exe");
        assert!(app_key_matches("win-exe:eldenring.exe", &focused));
        assert!(!app_key_matches("win-exe:notepad.exe", &focused));
    }

    #[test]
    fn non_matching_keys_are_false() {
        let focused = AppKey::windows_exe("C:\\notepad.exe");
        assert!(!app_key_matches("win-exe:c:\\brave.exe", &focused));
    }

    #[test]
    fn consecutive_samples_of_one_app_collapse_into_one_observation() {
        let mut acc = ObsAccumulator::default();
        assert!(acc
            .offer(sample(Some(exe("C:\\a\\g.exe"))), at(100))
            .is_empty());
        assert!(acc
            .offer(sample(Some(exe("C:\\a\\g.exe"))), at(103))
            .is_empty());
        // Same app, different path spelling: identity is the canonical key,
        // so it still collapses rather than splitting the window.
        assert!(acc
            .offer(sample(Some(exe("C:\\A\\G.EXE"))), at(105))
            .is_empty());
    }

    #[test]
    fn crossing_the_window_edge_flushes_and_realigns_to_the_clock_grid() {
        let mut acc = ObsAccumulator::default();
        acc.offer(sample(Some(exe("C:\\a\\g.exe"))), at(95));
        // t=105 is past the window ending at t=100: the window closes stamped
        // at its grid edge, and a fresh window opens for 110.
        let out = acc.offer(sample(Some(exe("C:\\a\\g.exe"))), at(105));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].observed_at_utc, "1970-01-01T00:01:40Z");
    }

    #[test]
    fn an_app_change_flushes_the_open_window_and_starts_a_new_one() {
        let mut acc = ObsAccumulator::default();
        acc.offer(sample(Some(exe("C:\\a\\g.exe"))), at(96));
        let out = acc.offer(sample(Some(exe("C:\\b\\w.exe"))), at(97));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].app_key, exe("C:\\a\\g.exe"));
        // The new app's window stays open.
        assert!(acc
            .offer(sample(Some(exe("C:\\b\\w.exe"))), at(98))
            .is_empty());
        let drained = acc.flush(at(99));
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].app_key, exe("C:\\b\\w.exe"));
    }

    #[test]
    fn an_idle_transition_flushes_the_open_window() {
        let mut acc = ObsAccumulator::default();
        let mut active = sample(Some(exe("C:\\a\\g.exe")));
        active.idle_seconds = 0;
        acc.offer(active, at(96));
        let mut away = sample(Some(exe("C:\\a\\g.exe")));
        away.idle_seconds = 300;
        let out = acc.offer(away, at(97));
        assert_eq!(out.len(), 1, "presence flip must close the window");
        // The closing window held only active readings, so it reports its own
        // last value; the away reading belongs to the window it opens.
        assert_eq!(out[0].idle_seconds, 0);
        let drained = acc.flush(at(98));
        assert_eq!(drained.len(), 1);
        assert_eq!(
            drained[0].idle_seconds, 300,
            "the away reading lands in the fresh window"
        );
    }

    #[test]
    fn losing_focus_credits_nobody_but_flushes_the_open_window() {
        let mut acc = ObsAccumulator::default();
        acc.offer(sample(Some(exe("C:\\a\\g.exe"))), at(96));
        let out = acc.offer(sample(None), at(97));
        assert_eq!(out.len(), 1);
        assert!(acc.offer(sample(None), at(98)).is_empty());
        assert!(
            acc.flush(at(99)).is_empty(),
            "no phantom desktop observation"
        );
    }

    #[test]
    fn observation_stamps_are_strictly_ascending_across_windows() {
        let mut acc = ObsAccumulator::default();
        let mut stamps = Vec::new();
        // Alternate apps each tick so a window closes on every offer.
        for t in [96i64, 107, 118, 129] {
            let path = if (t / 10) % 2 == 0 {
                "C:\\a\\g.exe"
            } else {
                "C:\\b\\w.exe"
            };
            stamps.extend(acc.offer(sample(Some(exe(path))), at(t)));
        }
        stamps.extend(acc.flush(at(130)));
        assert!(stamps.len() >= 4);
        for pair in stamps.windows(2) {
            assert!(
                pair[0].observed_at_utc.as_bytes() < pair[1].observed_at_utc.as_bytes(),
                "stamps must ascend: {} then {}",
                pair[0].observed_at_utc,
                pair[1].observed_at_utc
            );
        }
    }

    #[test]
    fn the_window_title_comes_from_the_first_titled_sample_only() {
        let mut acc = ObsAccumulator::default();
        let mut s1 = sample(Some(exe("C:\\a\\g.exe")));
        s1.title = Some("Boss fight".into());
        acc.offer(s1, at(96));
        let mut s2 = sample(Some(exe("C:\\a\\g.exe")));
        s2.title = Some("Different tab".into());
        assert!(acc.offer(s2, at(97)).is_empty());
        let out = acc.flush(at(99));
        assert_eq!(out[0].window_title.as_deref(), Some("Boss fight"));
    }

    #[test]
    fn a_partial_window_flushed_at_shutdown_is_stamped_at_the_actual_instant() {
        let mut acc = ObsAccumulator::default();
        acc.offer(sample(Some(exe("C:\\a\\g.exe"))), at(96));
        let out = acc.flush(at(98));
        assert_eq!(out[0].observed_at_utc, "1970-01-01T00:01:38Z");
    }

    fn blocked(id: i64, key: &str) -> BlockedAppDto {
        BlockedAppDto {
            app_id: id,
            label: format!("app {id}"),
            app_key: key.into(),
        }
    }

    fn facts<'a>(
        focused: Option<&'a AppKey>,
        changed: bool,
        cache_valid: bool,
        active: Option<i64>,
    ) -> CycleFacts<'a> {
        CycleFacts {
            focused_key: focused,
            focused_key_changed: changed,
            cache_valid,
            pin_configured: false,
            overlay_active_app: active,
        }
    }

    #[test]
    fn a_blocked_focus_shows_an_overlay() {
        let g = exe("C:\\a\\g.exe");
        let plan = plan_cycle(
            facts(Some(&g), true, false, None),
            &[blocked(6, "win-exe:g.exe")],
        );
        assert_eq!(
            plan.overlay,
            OverlayCmd::Show {
                app_id: 6,
                gate: QuitGate::ButtonsAvailable,
            }
        );
        assert!(plan.fetch_blocked);
    }

    #[test]
    fn the_same_block_keeps_the_current_overlay() {
        let g = exe("C:\\a\\g.exe");
        let plan = plan_cycle(
            facts(Some(&g), false, true, Some(6)),
            &[blocked(6, "win-exe:g.exe")],
        );
        assert_eq!(plan.overlay, OverlayCmd::Stay);
        assert!(!plan.fetch_blocked);
    }

    #[test]
    fn a_different_block_switches_overlays() {
        let w = exe("C:\\b\\w.exe");
        let plan = plan_cycle(
            facts(Some(&w), true, true, Some(6)),
            &[blocked(6, "win-exe:g.exe"), blocked(7, "win-exe:w.exe")],
        );
        assert_eq!(
            plan.overlay,
            OverlayCmd::Show {
                app_id: 7,
                gate: QuitGate::ButtonsAvailable,
            }
        );
    }

    #[test]
    fn losing_the_block_or_focus_dismisses_the_overlay() {
        let g = exe("C:\\a\\g.exe");
        // Block lifted while still focused on the app…
        let plan = plan_cycle(facts(Some(&g), false, true, Some(6)), &[]);
        assert_eq!(plan.overlay, OverlayCmd::Dismiss);
        // …and focus moved to something not blocked.
        let n = exe("C:\\c\\n.exe");
        let plan = plan_cycle(
            facts(Some(&n), true, true, Some(6)),
            &[blocked(6, "win-exe:g.exe")],
        );
        assert_eq!(plan.overlay, OverlayCmd::Dismiss);
    }

    #[test]
    fn an_invalid_or_stale_cache_requests_a_refresh() {
        let g = exe("C:\\a\\g.exe");
        // Never fetched…
        let plan = plan_cycle(facts(Some(&g), false, false, None), &[]);
        assert!(plan.fetch_blocked);
        // …or fetched but older than BLOCKEDAPPS_MAX_AGE_SECS (main folds the
        // age check into `cache_valid` before calling).
        let plan = plan_cycle(facts(Some(&g), false, false, None), &[blocked(6, "x")]);
        assert!(plan.fetch_blocked);
    }

    #[test]
    fn a_fresh_cache_with_stable_focus_skips_the_refresh() {
        let g = exe("C:\\a\\g.exe");
        let plan = plan_cycle(facts(Some(&g), false, true, None), &[]);
        assert!(!plan.fetch_blocked);
    }

    #[test]
    fn a_focus_change_requests_a_refresh_even_when_the_cache_is_fresh() {
        let g = exe("C:\\a\\g.exe");
        let plan = plan_cycle(facts(Some(&g), true, true, None), &[]);
        assert!(plan.fetch_blocked);
    }

    #[test]
    fn a_configured_pin_locks_quit_behind_pin_mode() {
        // Through the planner: the Show command must carry the locked gate.
        let g = exe("C:\\a\\g.exe");
        let mut f = facts(Some(&g), true, false, None);
        f.pin_configured = true;
        let plan = plan_cycle(f, &[blocked(6, "win-exe:g.exe")]);
        assert_eq!(
            plan.overlay,
            OverlayCmd::Show {
                app_id: 6,
                gate: QuitGate::PinLocked,
            }
        );
        // And the mapping itself.
        assert_eq!(quit_gate(true), QuitGate::PinLocked);
    }

    #[test]
    fn no_pin_keeps_quit_available() {
        assert_eq!(quit_gate(false), QuitGate::ButtonsAvailable);
        // Unknown status must fail safe: no Quit button until proven absent.
        assert_ne!(quit_gate(false), quit_gate(true));
    }
}
