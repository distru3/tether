//! Linux enforcement: a stub.
//!
//! As on Windows, blocking is the block screen, not anything done to the
//! app's processes; this crate will only close an app on request and write
//! `/etc/hosts`. The design is `docs/adr/0004-linux-port.md` (phases L1 and
//! L3). Follow that ADR, not older notes.

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
            "/proc scanning is not implemented yet (docs/adr/0004-linux-port.md)",
        ))
    }

    fn terminate(&mut self, _pid: u32) -> PlatformResult<()> {
        Err(PlatformError::Unsupported(
            "process termination is not implemented yet (docs/adr/0004-linux-port.md)",
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
            "/etc/hosts filtering is not implemented yet (docs/adr/0004-linux-port.md)",
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
