//! Per-interface DNS capture and override.
//!
//! This is the one place in the DNS filter that is genuinely system-stateful:
//! it reads the machine's DNS configuration and, while the filter is active,
//! points every active interface's IPv4 resolver at `127.0.0.1` so that all
//! resolver traffic lands on our local responder.
//!
//! # How it works
//!
//! * **Capture** uses [`GetAdaptersAddresses`] (iphlpapi) with the unicast /
//!   anycast / multicast address lists skipped — we only care about the DNS
//!   server chain and the friendly name. Only adapters whose `OperStatus` is
//!   `IfOperStatusUp` are captured, so a dead VPN adapter is not turned into a
//!   `127.0.0.1` resolver.
//! * **Override** runs `netsh interface ipv4 set dnsservers ...` per adapter.
//!   `validate=no` is essential: in SYSTEM context `netsh` would otherwise try
//!   to validate reachability and can hang on an unresponsive resolver.
//! * **Restore** replays the captured addresses (or `source=dhcp` when the
//!   adapter had none) on `clear()`, so an uninstall returns the machine to its
//!   prior state.
//!
//! # Honest caveats (verified by manual integration, not headless tests)
//!
//! * The override is global to the machine and lives outside the process; a
//!   crash between override and restore leaves the machine pointed at
//!   `127.0.0.1`. The agent's fail-closed rule — blocks persist across restarts
//!   and startup never wipes them — means the resolver is re-applied on next
//!   start, which is the same state. `clear()` is the undo.
//! * Restore pins the captured static values even when they originally came
//!   from DHCP (the API reports DHCP-assigned DNS in the same list). That pins
//!   today's values rather than tracking future DHCP changes; it never strands
//!   the machine, and it is the documented cost of not reading the DHCP flag.
//! * On failure to capture, the caller must NOT override anything — see
//!   [`DnsProxyFilter`](crate::filter::DnsProxyFilter). Never override what we
//!   did not first observe.

use std::net::{IpAddr, Ipv4Addr};

use st_core::platform::{PlatformError, PlatformResult};
use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS};
use windows::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST, GAA_FLAG_SKIP_UNICAST,
    IP_ADAPTER_ADDRESSES_LH, IP_ADAPTER_DNS_SERVER_ADDRESS_XP,
};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Networking::WinSock::{AF_INET, IN_ADDR, SOCKADDR_IN};

/// One interface's IPv4 DNS configuration, as it was before we touched it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfaceDns {
    /// The interface's friendly name (what `netsh` wants in `name="..."`).
    pub name: String,
    /// The IPv4 resolver addresses in use, in order. Empty means "DHCP".
    pub servers: Vec<IpAddr>,
}

/// Capture the currently-active IPv4 DNS configuration for every up interface.
///
/// Returns `Err` on failure so the caller can refuse to override without
/// stranding the machine; this is the only call that MUST succeed before any
/// override is attempted.
pub fn capture() -> PlatformResult<Vec<IfaceDns>> {
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_UNICAST;
    let family = AF_INET.0 as u32;

    // First call sizes the buffer. Passing a null pointer with `size = 0` is the
    // documented way to probe; it returns ERROR_BUFFER_OVERFLOW and fills in
    // the required byte count.
    let mut size: u32 = 0;
    let first = unsafe { GetAdaptersAddresses(family, flags, None, None, &mut size) };
    if first != ERROR_BUFFER_OVERFLOW.0 {
        return Err(PlatformError::Other(format!(
            "GetAdaptersAddresses probe returned error code {first}"
        )));
    }

    // u64-aligned heap buffer (the struct is 8-byte aligned); round up so the
    // size strictly fits.
    let mut buf: Vec<u64> = vec![0; (size as usize).div_ceil(8)];
    let head = buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
    let second = unsafe { GetAdaptersAddresses(family, flags, None, Some(head), &mut size) };
    if second != ERROR_SUCCESS.0 {
        return Err(PlatformError::Other(format!(
            "GetAdaptersAddresses failed with error code {second}"
        )));
    }

    let mut out = Vec::new();
    // SAFETY: `head` is a heap pointer to a buffer that was sized (and confirmed
    // re-sized) by the API, and every `Next` pointer is at `IP_ADAPTER_ADDRESSES_LH`
    // boundaries inside that buffer, terminated by null. We only read fields and
    // hold no reference past the buffer's lifetime here.
    unsafe {
        let mut current: *mut IP_ADAPTER_ADDRESSES_LH = head;
        while !current.is_null() {
            let adapter = &*current;
            // Skip loopback interfaces (IF_TYPE_SOFTWARE_LOOPBACK = 24)
            if adapter.OperStatus == IfOperStatusUp && adapter.IfType != 24 {
                // `FriendlyName` is a wide (UTF-16) string; read it lossily so an
                // odd but valid adapter name still yields a net-ish label.
                let name = if adapter.FriendlyName.is_null() {
                    String::new()
                } else {
                    String::from_utf16_lossy(adapter.FriendlyName.as_wide())
                };
                let servers = dns_servers(adapter.FirstDnsServerAddress);
                if !name.is_empty() {
                    out.push(IfaceDns { name, servers });
                }
            }
            current = adapter.Next;
        }
    }
    Ok(out)
}

/// Walk an adapter's DNS server chain and collect its IPv4 resolvers.
unsafe fn dns_servers(head: *mut IP_ADAPTER_DNS_SERVER_ADDRESS_XP) -> Vec<IpAddr> {
    let mut servers = Vec::new();
    let mut current = head;
    while !current.is_null() {
        let entry = &*current;
        // `SOCKET_ADDRESS.lpSockaddr` is a `SOCKADDR`; for an IPv4 entry the
        // family prefix lets us re-read it as `SOCKADDR_IN` and take the address.
        let sockaddr = entry.Address.lpSockaddr;
        if !sockaddr.is_null() {
            let addr_in = &*(sockaddr as *const SOCKADDR_IN);
            if addr_in.sin_family == AF_INET {
                servers.push(IpAddr::V4(sockaddr_to_ipv4(&addr_in.sin_addr)));
            }
        }
        current = entry.Next;
    }
    servers
}

/// Convert a net-byte-order `IN_ADDR` to an IPv4 octet string. `S_addr` is the
/// four bytes in network order read as a little-endian host integer, so the
/// low byte is the first octet.
fn sockaddr_to_ipv4(addr: &IN_ADDR) -> Ipv4Addr {
    let n = unsafe { addr.S_un.S_addr };
    Ipv4Addr::new(
        (n & 0xff) as u8,
        ((n >> 8) & 0xff) as u8,
        ((n >> 16) & 0xff) as u8,
        ((n >> 24) & 0xff) as u8,
    )
}

/// The first IPv4 resolver found across the (sorted) interface list, to use as
/// the forward-upstream. `None` means nothing usable was captured — the caller
/// then falls back to a public resolver rather than forwarding to ourselves.
pub fn pick_upstream(ifaces: &[IfaceDns]) -> Option<IpAddr> {
    ifaces
        .iter()
        .flat_map(|i| i.servers.iter())
        .copied()
        .find(|ip| !ip.is_loopback())
}

/// Point every captured interface's IPv4 resolver at `127.0.0.1`.
///
/// `validate=no` is load-bearing (see the module docs: SYSTEM hangs on
/// validation). Runs each `netsh` as a short-lived child; failure on one
/// interface is logged but does not abort the rest, so a single stubborn
/// adapter cannot undo the whole override.
pub fn override_all(ifaces: &[IfaceDns]) {
    for iface in ifaces {
        if let Err(e) = set_servers(&iface.name, &[Ipv4Addr::LOCALHOST.into()]) {
            tracing::warn!(interface = %iface.name, error = %e, "DNS override failed for interface");
        }
    }
}

/// Restore each interface's captured DNS configuration.
///
/// Idempotent and safe to call with an empty snapshot: adapters with recorded
/// servers get them back statically; adapters with none go back to DHCP. A
/// failed restore is logged — the machine is still pointing at `127.0.0.1` (or
/// already at its original servers), and re-running clear/apply is the remedy.
pub fn restore_all(ifaces: &[IfaceDns]) {
    for iface in ifaces {
        let name_arg = format!("name={}", iface.name);
        let result = if iface.servers.is_empty() {
            command(
                "netsh",
                &[
                    "interface",
                    "ipv4",
                    "set",
                    "dnsservers",
                    &name_arg,
                    "source=dhcp",
                ],
            )
        } else {
            let first = &iface.servers[0];
            let addr_arg = format!("address={first}");
            let res = command(
                "netsh",
                &[
                    "interface",
                    "ipv4",
                    "set",
                    "dnsservers",
                    &name_arg,
                    "source=static",
                    &addr_arg,
                    "validate=no",
                ],
            );
            for sec in iface.servers.iter().skip(1) {
                let sec_addr_arg = format!("address={sec}");
                let _ = command(
                    "netsh",
                    &[
                        "interface",
                        "ipv4",
                        "add",
                        "dnsservers",
                        &name_arg,
                        &sec_addr_arg,
                        "validate=no",
                    ],
                );
            }
            res
        };
        if let Err(e) = result {
            tracing::warn!(interface = %iface.name, error = %e, "DNS restore failed for interface");
        }
    }
}

fn set_servers(name: &str, servers: &[IpAddr]) -> std::result::Result<(), String> {
    let Some(first) = servers.first() else {
        return Ok(());
    };
    let name_arg = format!("name={name}");
    let addr_arg = format!("address={first}");
    command(
        "netsh",
        &[
            "interface",
            "ipv4",
            "set",
            "dnsservers",
            &name_arg,
            "source=static",
            &addr_arg,
            "validate=no",
        ],
    )
}

/// Run an external command and capture a failure as a string, consuming stdout
/// for diagnostics. `netsh` is trusted to be on PATH on any Windows install.
fn command(program: &str, args: &[&str]) -> std::result::Result<(), String> {
    let out = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let msg = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        Err(format!(
            "{program} exited with {:?}: {msg}",
            out.status.code()
        ))
    }
}

/// Guard against the "no captured -> never override" invariant being broken by
/// a caller passing a stale list. Kept small and pure so it is testable.
pub fn has_captured(ifaces: &[IfaceDns]) -> bool {
    !ifaces.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_capture_is_detectable_and_never_upstream() {
        assert!(!has_captured(&[]));
        assert_eq!(pick_upstream(&[]), None);
    }

    #[test]
    fn pick_upstream_returns_the_first_server() {
        let ifaces = vec![IfaceDns {
            name: "Ethernet".into(),
            servers: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))],
        }];
        assert_eq!(
            pick_upstream(&ifaces),
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)))
        );
    }

    #[test]
    fn an_upstream_with_empty_servers_yields_nothing() {
        let ifaces = vec![IfaceDns {
            name: "Wi-Fi".into(),
            servers: vec![],
        }];
        assert_eq!(pick_upstream(&ifaces), None);
    }

    #[test]
    fn pick_upstream_skips_loopback_to_avoid_forwarding_loop() {
        let ifaces = vec![
            IfaceDns {
                name: "Wi-Fi".into(),
                servers: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
            },
            IfaceDns {
                name: "Ethernet".into(),
                servers: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))],
            },
        ];
        assert_eq!(
            pick_upstream(&ifaces),
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)))
        );
    }

    #[test]
    fn pick_upstream_returns_none_when_all_servers_are_loopback() {
        let ifaces = vec![IfaceDns {
            name: "Wi-Fi".into(),
            servers: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        }];
        assert_eq!(pick_upstream(&ifaces), None);
    }
}
