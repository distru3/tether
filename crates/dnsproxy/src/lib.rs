//! Family DNS: adapter DNS configuration and the browser DoH lockdown.
//!
//! Provides system DNS management for adult/malware protection (Cloudflare
//! Family DNS via `netsh`), capture of the original configuration for exact
//! restoration, and browser enterprise policies plus DoT firewall rules so
//! browsers cannot bypass system DNS. Orchestration (backups, ordering,
//! rollback) lives in the agent's `family_dns` module.
//!
//! # Module map
//!
//! * [`dns_config`] (Windows) — per-interface DNS capture and restore via
//!   `GetAdaptersAddresses` + `netsh` (Cloudflare Family DNS).
//! * [`lockdown`] — browser enterprise policy keys to disable DoH and ensure
//!   queries respect system DNS resolution; outbound DoT firewall rules.
//!
//! Domain blocking itself is not here: it is the hosts file
//! (`st-enforce-win::HostsFileFilter`).

pub mod lockdown;

#[cfg(windows)]
pub mod dns_config;
