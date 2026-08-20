//! Linux enforcement.
//!
//! # Status: stub, pending the M0 spike
//!
//! # Implementation notes for the spike
//!
//! **Freezing.** Prefer the cgroup v2 freezer over signals: write `1` to
//! `cgroup.freeze` in the app's cgroup and the entire process tree stops
//! atomically, with no risk of a child escaping between two `SIGSTOP`s. Fall
//! back to `SIGSTOP`/`SIGCONT` via `nix` when the cgroup is not writable.
//!
//! Note that `SIGSTOP` on a desktop app can leave the compositor waiting on a
//! frozen client, which looks like a system-wide hang. The cgroup freezer plus
//! an overlay window that grabs focus first avoids this.
//!
//! **Process discovery.** Walk `/proc`, read `exe` symlinks. For Flatpak and
//! Snap, match on `/proc/<pid>/cgroup` instead, since every sandboxed app has
//! the same `bwrap` executable.
//!
//! **Domain filtering.** `/etc/hosts` mirrors the Windows implementation and
//! shares its two fatal weaknesses (no wildcards, ignored by DNS-over-HTTPS).
//! The real filter is the M3 DNS proxy, with `nftables` closing ports 53 and
//! 853 to everything except the local resolver.

use st_core::model::AppKey;
use st_core::platform::{
    BlockRule, FilterCapabilities, NetworkFilter, PlatformError, PlatformResult, ProcessController,
};

#[derive(Default)]
pub struct CgroupProcessController;

impl CgroupProcessController {
    pub fn new() -> Self {
        Self
    }
}

impl ProcessController for CgroupProcessController {
    fn find_processes(&mut self, _key: &AppKey) -> PlatformResult<Vec<u32>> {
        Err(PlatformError::Unsupported(
            "/proc scanning is not implemented yet (M0 spike)",
        ))
    }

    fn freeze(&mut self, _pid: u32) -> PlatformResult<()> {
        Err(PlatformError::Unsupported(
            "cgroup v2 freezer is not implemented yet (M0 spike)",
        ))
    }

    fn thaw(&mut self, _pid: u32) -> PlatformResult<()> {
        Err(PlatformError::Unsupported(
            "cgroup v2 freezer is not implemented yet (M0 spike)",
        ))
    }

    fn terminate(&mut self, _pid: u32) -> PlatformResult<()> {
        Err(PlatformError::Unsupported(
            "process termination is not implemented yet (M0 spike)",
        ))
    }

    fn backend(&self) -> &'static str {
        "linux-stub"
    }
}

#[derive(Default)]
pub struct EtcHostsFilter;

impl EtcHostsFilter {
    pub fn new() -> Self {
        Self
    }
}

impl NetworkFilter for EtcHostsFilter {
    fn apply(&mut self, _rules: &[BlockRule]) -> PlatformResult<()> {
        Err(PlatformError::Unsupported(
            "/etc/hosts filtering is not implemented yet (M0 spike)",
        ))
    }

    fn clear(&mut self) -> PlatformResult<()> {
        // Clearing must always succeed: it runs on uninstall, and refusing to
        // clean up would leave the user's machine filtered with no way to undo
        // it from the app.
        Ok(())
    }

    fn capabilities(&self) -> FilterCapabilities {
        FilterCapabilities {
            wildcard_domains: false,
            blocks_encrypted_dns: false,
            path_level: false,
        }
    }

    fn backend(&self) -> &'static str {
        "linux-stub"
    }
}
