//! DNS configuration and network filtering utilities.
//!
//! Provides system DNS management for adult/malware protection (Cloudflare Family DNS
//! via `netsh`), original DNS configuration backup and restoration, browser DoH policy
//! lockdown, and network filter hooks.
//!
//! # Module map
//!
//! * [`dns_config`] (Windows) — per-interface DNS capture and restore via
//!   `GetAdaptersAddresses` + `netsh` (Cloudflare Family DNS).
//! * [`lockdown`] — browser enterprise policy keys to disable DoH and ensure queries
//!   respect system DNS resolution.
//! * [`filter`] (Windows) — [`DnsProxyFilter`] network filter adapter.
//!
//! On non-Windows targets the OS-touching modules compile away and the crate
//! is a transparent no-op (mirroring `st-enforce-win`), so the workspace builds
//! everywhere while the resolver logic stays fully exercised by tests.

pub mod lockdown;

#[cfg(windows)]
pub mod dns_config;
#[cfg(windows)]
pub mod filter;

#[cfg(windows)]
pub use filter::DnsProxyFilter;
