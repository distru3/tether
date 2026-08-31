//! [`DnsProxyFilter`]: the [`NetworkFilter`] the agent actually holds for
//! Tier 2 DNS blocking.
//!
//! # State machine
//!
//! The filter is deliberately tolerant of its environment failing:
//!
//! * **`apply(rules)`**
//!   1. (once, ever) capture the machine's current per-interface DNS servers.
//!      If the capture fails, we **refuse to override anything** — a machine we
//!      never observed is never pointed somewhere we cannot undo (never strand
//!      a machine with no resolver).
//!   2. store the rules into the resolver's shared set (thread-safe, so a query
//!      in flight sees the new set immediately after this returns).
//!   3. ensure the UDP listener on `127.0.0.1:53` is running. If the bind fails
//!      (dev console without elevation, port already claimed, firewall), we log
//!      loudly and set a degraded flag rather than crashing — the agent keeps
//!      running and the `hosts` filter that shares the composite keeps blocking.
//!   4. override each active interface's IPv4 resolver to `127.0.0.1`.
//!
//!   `apply` is idempotent: re-applying the same (or new) rules re-stores the
//!   set and re-runs the (idempotent) netsh override without tearing down the
//!   listener.
//!
//! * **`clear()`** — idempotent and always safe, even if nothing was ever
//!   applied: stop the listener if any, restore the captured DNS servers, empty
//!   the rule set, and reset the capture so a later apply starts fresh.
//!
//! # Capability honesty
//!
//! `capabilities()` reports `wildcard_domains: true` (this backend really does
//! enforce subdomains — that is its whole reason to exist), `blocks_encrypted_dns:
//! false` (DoH/DoT lockdown is Phase 2) and `path_level: false` (DNS cannot see
//! URLs). A bind failure is surfaced through logs and the agent's status, not by
//! pretending the backend is more capable than it is.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};

use st_core::platform::{BlockRule, FilterCapabilities, NetworkFilter, PlatformResult};

use crate::dns_config::{self, IfaceDns};
use crate::server::Resolver;

/// Fallback upstream when capture yields nothing usable. A public resolver is
/// the last resort: the alternative (forwarding to `127.0.0.1`) would loop.
const DEFAULT_UPSTREAM: Ipv4Addr = Ipv4Addr::new(8, 8, 8, 8);

/// The local resolver address the responder binds and interfaces are pointed at.
pub const LOCAL_RESOLVER_ADDR: &str = "127.0.0.1:53";

#[must_use]
pub struct DnsProxyFilter {
    /// The effective rule set shared with the resolver thread.
    rules: Arc<RwLock<Vec<BlockRule>>>,
    /// The upstream the resolver forwards to. Assigned on first capture; the
    /// value is what keeps us from forwarding to ourselves.
    upstream: SocketAddr,
    resolver: Option<Resolver>,
    /// False when the bind failed and the listener is inert.
    listener_up: bool,
    /// The DNS config captured once, on first apply. `None` also covers "never
    /// captured yet" and "capture failed" — the boolean is what stops retries.
    captured: Option<Vec<IfaceDns>>,
    /// Whether we have already attempted capture, so a failed capture is not
    /// retried on every subsequent apply.
    capture_attempted: bool,
    /// Whether browser DoH enterprise registry policy lockdown is currently effective.
    doh_locked_down: bool,
}

impl Default for DnsProxyFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl DnsProxyFilter {
    pub fn new() -> Self {
        Self {
            rules: Arc::new(RwLock::new(Vec::new())),
            upstream: SocketAddr::new(IpAddr::V4(DEFAULT_UPSTREAM), 53),
            resolver: None,
            listener_up: false,
            captured: None,
            capture_attempted: false,
            doh_locked_down: false,
        }
    }

    /// Replace the shared rule set. Up to the caller to supply the allow-resolved
    /// blocks; the authoritative shape is `DnsProxyFilter::apply`'s contract.
    fn set_rules(&self, rules: &[BlockRule]) {
        let mut set = self
            .rules
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        set.clear();
        set.extend_from_slice(rules);
    }

    /// Ensure the listener is up, degrading (not crashing) on bind failure.
    fn ensure_listener(&mut self) {
        if self.resolver.is_some() {
            return;
        }
        match Resolver::start(LOCAL_RESOLVER_ADDR, self.upstream, self.rules.clone()) {
            Ok(resolver) => {
                self.listener_up = true;
                self.resolver = Some(resolver);
                tracing::info!(upstream = %self.upstream, "DNS resolver listening on 127.0.0.1:53");
            }
            Err(e) => {
                self.listener_up = false;
                tracing::error!(
                    error = %e,
                    "DNS resolver could not bind 127.0.0.1:53; DNS proxy is inert \
                     (hosts filter still active). Elevated/console-mode agents and free port 53 \
                     are required."
                );
            }
        }
    }
}

impl NetworkFilter for DnsProxyFilter {
    fn apply(&mut self, rules: &[BlockRule]) -> PlatformResult<()> {
        if rules.is_empty() {
            // When no rules are active (e.g. user toggled off all blocklists),
            // cleanly restore the machine's DNS, remove firewall rules/lockdown,
            // and stop the local resolver listener.
            return self.clear();
        }

        self.set_rules(rules);

        // Capture the machine's DNS once. Capture must succeed before we even
        // consider overriding, or we could strand a machine we never observed.
        if !self.capture_attempted {
            self.capture_attempted = true;
            match dns_config::capture() {
                Ok(ifaces) => {
                    if let Some(upstream) = dns_config::pick_upstream(&ifaces) {
                        self.upstream = SocketAddr::new(upstream, 53);
                    }
                    self.captured = Some(ifaces);
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "DNS capture failed; refusing to override anything (machine left as found)"
                    );
                }
            }
        }

        self.ensure_listener();

        // CRITICAL: Only override interfaces when the listener is actually up.
        // Pointing DNS at 127.0.0.1:53 when nothing is listening there causes
        // total DNS failure — no websites load, no recovery possible without
        // Safe Mode.
        if self.listener_up {
            if let Some(ifaces) = &self.captured {
                dns_config::override_all(ifaces);
            }
        } else {
            tracing::warn!(
                "DNS listener is not up; skipping DNS override to avoid stranding the machine"
            );
        }

        // Apply browser DoH policy lockdown and firewall rules.
        let lockdown = crate::lockdown::apply_lockdown(Some(self.upstream.ip()));
        self.doh_locked_down = lockdown.is_effective();

        Ok(())
    }

    fn clear(&mut self) -> PlatformResult<()> {
        // Stop the listener first so no query can race the restore.
        if let Some(mut resolver) = self.resolver.take() {
            resolver.stop();
        }
        self.listener_up = false;

        if let Some(ifaces) = self.captured.take() {
            dns_config::restore_all(&ifaces);
        }
        crate::lockdown::clear_lockdown();
        self.doh_locked_down = false;
        self.capture_attempted = false;
        self.set_rules(&[]);
        Ok(())
    }

    fn capabilities(&self) -> FilterCapabilities {
        FilterCapabilities {
            wildcard_domains: true,
            blocks_encrypted_dns: self.doh_locked_down,
            path_level: false,
        }
    }

    fn backend(&self) -> &'static str {
        "dns-proxy"
    }
}
