//! The only place in the agent that knows which operating system it is on.
//!
//! Every other module works through the `st-core` traits. Adding or removing a
//! platform is a change to this file plus a `Cargo.toml` entry.

use st_core::platform::{IdleMonitor, NetworkFilter, ProcessController, WindowTracker};

pub struct Backends {
    pub tracker: Box<dyn WindowTracker>,
    pub idle: Box<dyn IdleMonitor>,
    /// The process controller is used only by the IPC server now (for the
    /// overlay's "Quit" action), so it is optional and moved out of the
    /// sampler's backends at startup.
    pub processes: Option<Box<dyn ProcessController>>,
    pub filter: Box<dyn NetworkFilter>,
}

#[cfg(windows)]
pub fn detect(capture_titles: bool, idle_threshold_secs: u64) -> Backends {
    use st_enforce_win::{HostsFileFilter, Win32ProcessController};
    use st_tracker_win::{Win32IdleMonitor, Win32WindowTracker};

    Backends {
        tracker: Box::new(Win32WindowTracker::new(capture_titles)),
        idle: Box::new(Win32IdleMonitor::new(idle_threshold_secs)),
        processes: Some(Box::new(Win32ProcessController::new())),
        filter: Box::new(HostsFileFilter::new()),
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn detect(_capture_titles: bool, idle_threshold_secs: u64) -> Backends {
    use st_enforce_linux::{CgroupProcessController, EtcHostsFilter};
    use st_tracker_linux::{is_x11_session, X11IdleMonitor, X11WindowTracker};

    if !is_x11_session() {
        tracing::warn!(
            "not an X11 session: active-window tracking is unavailable. \
             Wayland support is out of scope for v1."
        );
    }

    Backends {
        tracker: Box::new(X11WindowTracker::new()),
        idle: Box::new(X11IdleMonitor::new(idle_threshold_secs)),
        processes: Some(Box::new(CgroupProcessController::new())),
        filter: Box::new(EtcHostsFilter::new()),
    }
}
