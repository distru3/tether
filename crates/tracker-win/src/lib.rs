//! Windows implementations of [`WindowTracker`](st_core::platform::WindowTracker)
//! and [`IdleMonitor`](st_core::platform::IdleMonitor).
//!
//! Windows is the reference platform: it is the one that must always work.
//!
//! On non-Windows targets this crate compiles to nothing, so the workspace
//! still builds on Linux.

#[cfg(windows)]
mod win;

#[cfg(windows)]
pub mod discovery;

#[cfg(windows)]
pub use discovery::scan_installed_apps;
#[cfg(windows)]
pub use win::{Win32IdleMonitor, Win32WindowTracker};

#[cfg(not(windows))]
pub fn scan_installed_apps() -> Vec<st_ipc::DiscoveredAppDto> {
    Vec::new()
}
