//! DNS-proxy domain filtering: the Tier 2 network filter.
//!
//! The [`HostsFileFilter`](st_enforce_win) in Tier 1 matches exact names only,
//! and any browser with DNS-over-HTTPS enabled walks straight past it. This
//! crate is the real filter: a local resolver on `127.0.0.1:53` that answers
//! `NXDOMAIN` for blocked domains and forwards everything else to the machine's
//! real resolver, layered on top of the `hosts` filter to close the wildcard
//! gap that `hosts` cannot express.
//!
//! # Module map
//!
//! * [`resolve`] — pure, cross-platform decision logic: given a domain and the
//!   effective block rules, `Block` or `Forward`. This is the only thing the
//!   DNS server consults; it is unit-testable without a socket.
//! * [`wire`] — pure DNS wire-format helpers: parse a query header + QNAME and
//!   build an `NXDOMAIN` reply. Kept free of I/O so the byte layout is testable
//!   headless.
//! * [`server`] (Windows) — the UDP responder thread: bind `127.0.0.1:53`,
//!   receive a query, run [`resolve::decision`], reply or forward.
//! * [`filter`] (Windows) — [`DnsProxyFilter`]: the [`NetworkFilter`] the agent
//!   actually holds. Owns the shared rule set, the listener lifecycle, and the
//!   system DNS capture/override.
//! * [`dns_config`] (Windows) — per-interface DNS capture and restore via
//!   `GetAdaptersAddresses` + `netsh`. System-stateful by design; verified by
//!   manual integration, not headless tests.
//!
//! On non-Windows targets the OS-touching modules compile away and the crate
//! is a transparent no-op (mirroring `st-enforce-win`), so the workspace builds
//! everywhere while the resolver logic stays fully exercised by tests.

pub mod lockdown;
pub mod resolve;

#[cfg(windows)]
pub mod dns_config;
#[cfg(windows)]
pub mod filter;
#[cfg(windows)]
pub mod server;
#[cfg(windows)]
pub mod wire;

#[cfg(windows)]
pub use filter::DnsProxyFilter;
