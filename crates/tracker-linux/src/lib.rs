//! Linux active-window and idle tracking: a stub.
//!
//! Every method returns [`PlatformError::Unsupported`], which the agent
//! reports as "tracking unavailable" instead of crashing. The design for the
//! real implementation (X11 directly; GNOME and KDE on Wayland through a Shell
//! extension and a KWin script) is `docs/adr/0004-linux-port.md`, phases L1
//! and L2. Follow that ADR, not older notes.

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
            "X11 active-window tracking is not implemented yet (docs/adr/0004-linux-port.md)",
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
            "XScreenSaver idle detection is not implemented yet (docs/adr/0004-linux-port.md)",
        ))
    }

    fn backend(&self) -> &'static str {
        "x11-stub"
    }
}
