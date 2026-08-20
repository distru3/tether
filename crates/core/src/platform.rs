//! The operating-system boundary.
//!
//! These four traits are the *only* way the rest of the application is allowed
//! to touch the OS. Implementations live in `st-tracker-win`,
//! `st-tracker-linux`, `st-enforce-win` and `st-enforce-linux`.
//!
//! Two payoffs:
//!
//! * Linux is disposable. If the X11 port turns out not to be worth the effort,
//!   deleting the Linux crates and one CI job is the whole job.
//! * The limits engine and sampler can be tested with fakes, no VM required.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::AppKey;

pub type PlatformResult<T> = Result<T, PlatformError>;

#[derive(Debug, Error)]
pub enum PlatformError {
    /// The current environment cannot support this operation at all — for
    /// example active-window tracking under a Wayland compositor with no
    /// suitable protocol. The UI must surface this as degraded functionality
    /// rather than pretending everything is fine.
    #[error("unsupported on this platform or desktop environment: {0}")]
    Unsupported(&'static str),

    /// The operation needs privileges the current process does not hold.
    #[error("insufficient privileges for {0}")]
    PermissionDenied(&'static str),

    /// The target process disappeared between discovery and action. Common and
    /// benign; callers should treat it as success.
    #[error("process {0} no longer exists")]
    ProcessGone(u32),

    #[error("os error during {context}: {source}")]
    Os {
        context: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("{0}")]
    Other(String),
}

impl PlatformError {
    pub fn os(context: &'static str, source: std::io::Error) -> Self {
        PlatformError::Os { context, source }
    }
}

/// The application the user is currently looking at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveWindow {
    pub key: AppKey,
    pub pid: u32,
    pub display_name: String,
    /// Window title. Privacy-sensitive (it leaks document names, page titles and
    /// sometimes credentials), so capture is opt-in and it is never persisted by
    /// default.
    pub title: Option<String>,
}

/// Whether the user is actually present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdleState {
    /// Input seen recently, or a media-playback heuristic says the user is
    /// watching something.
    Active,
    Idle { for_secs: u64 },
    /// Session locked, or the screen is off.
    Locked,
}

impl IdleState {
    /// Only `Active` time is credited against budgets.
    pub fn is_countable(self) -> bool {
        matches!(self, IdleState::Active)
    }
}

/// Reports which application currently has focus.
pub trait WindowTracker: Send {
    /// `Ok(None)` means "nothing has focus right now" (desktop, lock screen,
    /// compositor restart) — normal and not an error.
    fn active_window(&mut self) -> PlatformResult<Option<ActiveWindow>>;

    /// Human-readable backend name for diagnostics, e.g. `"win32"`, `"x11"`.
    fn backend(&self) -> &'static str;
}

/// Reports user presence.
pub trait IdleMonitor: Send {
    fn idle_state(&mut self) -> PlatformResult<IdleState>;
    fn backend(&self) -> &'static str;
}

/// Suspends and resumes applications that have hit a limit.
///
/// Freezing is strongly preferred over killing: a frozen process keeps its
/// unsaved work and can be thawed when the next day starts or an override is
/// granted. Terminating is the last resort, and only after the user has had a
/// grace countdown to save.
pub trait ProcessController: Send {
    /// All live PIDs belonging to an app, including child processes. Browsers
    /// and Electron apps are process trees, so returning only the "main" PID
    /// would leave the app running.
    fn find_processes(&mut self, key: &AppKey) -> PlatformResult<Vec<u32>>;

    fn freeze(&mut self, pid: u32) -> PlatformResult<()>;
    fn thaw(&mut self, pid: u32) -> PlatformResult<()>;

    /// Last resort. Callers must have shown a grace countdown first.
    fn terminate(&mut self, pid: u32) -> PlatformResult<()>;

    fn backend(&self) -> &'static str;
}

/// A domain-blocking rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockRule {
    pub domain: String,
    /// Also match subdomains. The `hosts`-file backend cannot honour this and
    /// must expand a known-subdomain list instead; the DNS-proxy backend
    /// supports it natively.
    pub include_subdomains: bool,
}

/// How thoroughly the active network backend can actually enforce a rule.
///
/// Surfaced in the UI so the product never overstates what it is doing. A
/// `hosts`-file-only install must not claim that adult content is blocked when
/// any browser with DNS-over-HTTPS enabled walks straight past it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterCapabilities {
    pub wildcard_domains: bool,
    /// Can shut down DNS-over-HTTPS and DNS-over-TLS bypass routes.
    pub blocks_encrypted_dns: bool,
    /// Can block individual URL paths, e.g. `youtube.com/shorts`. Only the
    /// browser extension can do this; DNS cannot see paths.
    pub path_level: bool,
}

/// Applies domain-level blocking.
pub trait NetworkFilter: Send {
    fn apply(&mut self, rules: &[BlockRule]) -> PlatformResult<()>;

    /// Remove everything this filter added and restore prior system state.
    /// Must be idempotent, and must run on uninstall.
    fn clear(&mut self) -> PlatformResult<()>;

    fn capabilities(&self) -> FilterCapabilities;
    fn backend(&self) -> &'static str;
}
