//! Network filter utilities for the agent runtime.

use st_core::platform::{BlockRule, FilterCapabilities, NetworkFilter, PlatformResult};

/// An inert [`NetworkFilter`] that does nothing. Used only to stand in for a
/// filter that has been moved behind a shared Arc (main moves the filter into
/// `Runtime` and leaves an inert one in `Backends` so the struct stays whole).
pub struct NoopNetworkFilter;

impl Default for NoopNetworkFilter {
    fn default() -> Self {
        Self
    }
}

impl NetworkFilter for NoopNetworkFilter {
    fn apply(&mut self, _rules: &[BlockRule]) -> PlatformResult<()> {
        Ok(())
    }
    fn clear(&mut self) -> PlatformResult<()> {
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
        "noop"
    }
}
