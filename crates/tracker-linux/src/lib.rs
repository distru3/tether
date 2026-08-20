//! X11 active-window and idle tracking for Linux.
//!
//! # Status: stub, pending the M0 spike
//!
//! Every method returns [`PlatformError::Unsupported`], which the agent handles
//! by reporting degraded tracking rather than crashing. This is intentional:
//! Linux is a secondary target and may be cancelled outright, so no effort is
//! spent here until the spike proves it is worth it.
//!
//! # Implementation notes for the spike
//!
//! **Active window (X11).** Add `x11rb`, connect via `x11rb::connect(None)`,
//! then read `_NET_ACTIVE_WINDOW` from the root window and `_NET_WM_PID` from
//! the resulting window. Resolve the PID through `/proc/<pid>/exe`. Note that
//! `_NET_WM_PID` is advisory and some clients omit it, in which case fall back
//! to matching `WM_CLASS` against installed `.desktop` entries.
//!
//! **Idle (X11).** The XScreenSaver extension's `XScreenSaverQueryInfo` gives
//! idle milliseconds directly. Prefer it over polling input devices.
//!
//! **Sandboxed apps.** Flatpak and Snap processes all look like `bwrap` at the
//! process level, so `/proc/<pid>/cgroup` must be parsed to recover the real
//! application id. Without this, every Flatpak app collapses into one entry and
//! per-app limits are meaningless.
//!
//! **Wayland is explicitly out of scope for v1.** There is no portable
//! active-window protocol: wlroots compositors expose
//! `wlr-foreign-toplevel-management`, KDE exposes KWin's D-Bus scripting
//! interface, and GNOME exposes nothing usable without shipping a shell
//! extension. Under Wayland the agent must say so plainly in the UI instead of
//! silently reporting zeroes.

use st_core::platform::{
    ActiveWindow, IdleMonitor, IdleState, PlatformError, PlatformResult, WindowTracker,
};

/// Returns `true` when the process is running under X11 rather than Wayland.
///
/// Checked before construction so the UI can explain *why* tracking is
/// unavailable rather than just failing.
pub fn is_x11_session() -> bool {
    match std::env::var("XDG_SESSION_TYPE") {
        Ok(v) => v.eq_ignore_ascii_case("x11"),
        // No session-type hint: assume X11 if a display is set.
        Err(_) => std::env::var("DISPLAY").is_ok(),
    }
}

#[derive(Default)]
pub struct X11WindowTracker;

impl X11WindowTracker {
    pub fn new() -> Self {
        Self
    }
}

impl WindowTracker for X11WindowTracker {
    fn active_window(&mut self) -> PlatformResult<Option<ActiveWindow>> {
        Err(PlatformError::Unsupported(
            "X11 active-window tracking is not implemented yet (M0 spike)",
        ))
    }

    fn backend(&self) -> &'static str {
        "x11-stub"
    }
}

#[derive(Default)]
pub struct X11IdleMonitor;

impl X11IdleMonitor {
    pub fn new(_threshold_secs: u64) -> Self {
        Self
    }
}

impl IdleMonitor for X11IdleMonitor {
    fn idle_state(&mut self) -> PlatformResult<IdleState> {
        Err(PlatformError::Unsupported(
            "XScreenSaver idle detection is not implemented yet (M0 spike)",
        ))
    }

    fn backend(&self) -> &'static str {
        "x11-stub"
    }
}
