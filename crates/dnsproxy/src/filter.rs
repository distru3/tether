use st_core::platform::{BlockRule, FilterCapabilities, NetworkFilter, PlatformResult};

#[must_use]
#[derive(Default)]
pub struct DnsProxyFilter {}

impl DnsProxyFilter {
    pub fn new() -> Self {
        Self {}
    }
}

impl NetworkFilter for DnsProxyFilter {
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
        "dns-proxy"
    }
}
