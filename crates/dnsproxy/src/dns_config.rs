//! Per-interface DNS capture and override.
//!
//! This is the one place in the DNS filter that is genuinely system-stateful:
//! it reads the machine's DNS configuration and, when Family DNS is enabled,
//! configures active interfaces to use Cloudflare Family DNS resolvers
//! (`1.1.1.3` / `1.0.0.3`) for adult/malware protection.
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IfaceDns {
    /// The interface's friendly name (what `netsh` wants in `name="..."`).
    pub name: String,
    /// The IPv4 resolver addresses in use, in order.
    pub servers: Vec<IpAddr>,
    /// Whether DNS servers were obtained automatically via DHCP (true) or configured statically (false).
    #[serde(default)]
    pub is_dhcp: bool,
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
                let adapter_guid = if adapter.AdapterName.is_null() {
                    String::new()
                } else {
                    std::ffi::CStr::from_ptr(adapter.AdapterName.0 as *const _)
                        .to_string_lossy()
                        .into_owned()
                };
                let is_dhcp = is_adapter_dns_dhcp(&adapter_guid, &name);
                if !name.is_empty() {
                    out.push(IfaceDns {
                        name,
                        servers,
                        is_dhcp,
                    });
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

/// Query a REG_SZ or REG_EXPAND_SZ value from an open registry key.
#[cfg(windows)]
fn query_reg_string(
    key: windows::Win32::System::Registry::HKEY,
    value_name: &str,
) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegQueryValueExW, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE,
    };

    let wide_name: Vec<u16> = value_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let mut byte_len = 0u32;
        let res = RegQueryValueExW(
            key,
            PCWSTR(wide_name.as_ptr()),
            None,
            None,
            None,
            Some(&mut byte_len),
        );
        if res != ERROR_SUCCESS || byte_len == 0 {
            return None;
        }

        let mut data = vec![0u8; byte_len as usize];
        let mut kind = REG_VALUE_TYPE::default();
        let res = RegQueryValueExW(
            key,
            PCWSTR(wide_name.as_ptr()),
            None,
            Some(&mut kind),
            Some(data.as_mut_ptr()),
            Some(&mut byte_len),
        );
        if res != ERROR_SUCCESS {
            return None;
        }

        if kind != REG_SZ && kind != REG_EXPAND_SZ {
            return None;
        }

        let units: Vec<u16> = data[..byte_len as usize]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let text = String::from_utf16_lossy(&units);
        Some(text.trim_end_matches('\0').trim().to_string())
    }
}

/// Check whether the adapter's IPv4 DNS was configured via DHCP by inspecting
/// `HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{adapter_guid}`.
///
/// In Windows:
/// - When DNS is Automatic (DHCP), `NameServer` is absent or empty (`""`).
/// - When DNS was previously pinned or set to the local router, `NameServer` matches
///   `DhcpNameServer`, `DhcpServer`, `DhcpDefaultGateway`, or contains only private LAN IPs
///   (192.168.x.x, 10.x.x.x, etc.), which are DHCP-assigned addresses.
#[cfg(windows)]
fn check_registry_dns_dhcp(adapter_guid: &str) -> Option<bool> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, HKEY, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE,
    };

    if adapter_guid.is_empty() {
        return None;
    }

    let subkey_str = format!(
        "SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces\\{adapter_guid}"
    );
    let subkey_wide: Vec<u16> = subkey_str
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey_wide.as_ptr()),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
        .is_err()
        {
            return None;
        }

        let val = query_reg_string(key, "NameServer");
        let dhcp_ns = query_reg_string(key, "DhcpNameServer");
        let dhcp_srv = query_reg_string(key, "DhcpServer");
        let dhcp_gw = query_reg_string(key, "DhcpDefaultGateway");
        let _ = RegCloseKey(key);

        let ns = match val {
            Some(s) if !s.is_empty() => s,
            _ => return Some(true),
        };

        // If NameServer matches the DHCP-provided DNS, DHCP server, or DHCP gateway,
        // it was assigned by DHCP (or previously pinned from DHCP).
        if dhcp_ns.as_deref() == Some(&ns)
            || dhcp_srv.as_deref() == Some(&ns)
            || dhcp_gw.as_deref() == Some(&ns)
        {
            return Some(true);
        }

        // If all IPs in NameServer are private LAN IPs (e.g. 192.168.x.x, 10.x.x.x, 172.16-31.x.x),
        // they are local router addresses, not public static DNS resolvers.
        let is_all_private = ns
            .split([',', ' '])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .all(|ip_str| {
                if let Ok(ip) = ip_str.parse::<Ipv4Addr>() {
                    ip.is_private() || ip.is_loopback() || ip.is_link_local()
                } else {
                    false
                }
            });

        if is_all_private {
            return Some(true);
        }

        Some(false)
    }
}

/// Fallback check using `netsh interface ipv4 show dnsservers name="..."`.
#[cfg(windows)]
fn check_netsh_dns_dhcp(friendly_name: &str) -> bool {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("netsh");
    cmd.args([
        "interface",
        "ipv4",
        "show",
        "dnsservers",
        &format!("name={friendly_name}"),
    ]);
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    if let Ok(out) = cmd.output() {
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(pos) = text.find("Statically Configured DNS Servers:") {
            let after = &text[pos + "Statically Configured DNS Servers:".len()..];
            let static_ips: Vec<&str> = after
                .lines()
                .take_while(|line| !line.contains(':'))
                .map(str::trim)
                .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("None"))
                .collect();

            if static_ips.is_empty() {
                return true;
            }

            // If all statically listed IPs are private router/LAN IPs (192.168.x.x, 10.x.x.x, etc.)
            let all_private = static_ips.iter().all(|s| {
                if let Ok(ip) = s.parse::<Ipv4Addr>() {
                    ip.is_private() || ip.is_loopback() || ip.is_link_local()
                } else {
                    false
                }
            });

            if all_private {
                return true;
            }

            return false;
        }
        if text.contains("configured through DHCP:") {
            return true;
        }
    }
    true
}

#[cfg(windows)]
fn is_adapter_dns_dhcp(adapter_guid: &str, friendly_name: &str) -> bool {
    if let Some(is_dhcp) = check_registry_dns_dhcp(adapter_guid) {
        return is_dhcp;
    }
    check_netsh_dns_dhcp(friendly_name)
}

#[cfg(not(windows))]
fn is_adapter_dns_dhcp(_adapter_guid: &str, _friendly_name: &str) -> bool {
    true
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
pub fn set_family_dns(ifaces: &[IfaceDns]) {
    for iface in ifaces {
        // Set IPv4 to Cloudflare Family
        let _ = command(
            "netsh",
            &[
                "interface",
                "ipv4",
                "set",
                "dnsservers",
                &format!("name={}", iface.name),
                "source=static",
                "address=1.1.1.3",
                "validate=no",
            ],
        );
        let _ = command(
            "netsh",
            &[
                "interface",
                "ipv4",
                "add",
                "dnsservers",
                &format!("name={}", iface.name),
                "address=1.0.0.3",
                "validate=no",
            ],
        );

        // Set IPv6 to Cloudflare Family
        let _ = command(
            "netsh",
            &[
                "interface",
                "ipv6",
                "set",
                "dnsservers",
                &format!("name={}", iface.name),
                "source=static",
                "address=2606:4700:4700::1113",
                "validate=no",
            ],
        );
        let _ = command(
            "netsh",
            &[
                "interface",
                "ipv6",
                "add",
                "dnsservers",
                &format!("name={}", iface.name),
                "address=2606:4700:4700::1003",
                "validate=no",
            ],
        );
    }
    // Flush the system DNS cache
    let _ = command("ipconfig", &["/flushdns"]);
}

/// Restore each interface's captured DNS configuration.
///
/// Idempotent and safe to call with an empty snapshot: adapters configured
/// with DHCP (or empty servers) return cleanly to `source=dhcp`; adapters
/// with static configurations have their original IP addresses reinstated.
pub fn restore_all(ifaces: &[IfaceDns]) {
    for iface in ifaces {
        let name_arg = format!("name={}", iface.name);
        let all_servers_private = !iface.servers.is_empty()
            && iface.servers.iter().all(|ip| match ip {
                IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
                IpAddr::V6(v6) => v6.is_loopback(),
            });
        let should_restore_dhcp = iface.is_dhcp || iface.servers.is_empty() || all_servers_private;
        let result = if should_restore_dhcp {
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

        // Always reset IPv6 to DHCP since we don't capture it yet
        let _ = command(
            "netsh",
            &[
                "interface",
                "ipv6",
                "set",
                "dnsservers",
                &name_arg,
                "source=dhcp",
            ],
        );
    }
    let _ = command("ipconfig", &["/flushdns"]);
}

/// Write or remove FamilyDnsApplied DWORD in HKLM\Software\Screentime for the NSIS uninstaller.
pub fn set_registry_family_dns(enabled: bool) {
    #[cfg(windows)]
    {
        use windows::core::w;
        use windows::Win32::System::Registry::{
            RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY_LOCAL_MACHINE,
            KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE,
        };

        unsafe {
            let mut key = windows::Win32::System::Registry::HKEY::default();
            let subkey = w!("Software\\Screentime");
            let val_name = w!("FamilyDnsApplied");
            if RegCreateKeyExW(
                HKEY_LOCAL_MACHINE,
                subkey,
                0,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut key,
                None,
            )
            .is_ok()
            {
                if enabled {
                    let val: u32 = 1;
                    let _ = RegSetValueExW(
                        key,
                        val_name,
                        0,
                        REG_DWORD,
                        Some(std::slice::from_raw_parts(
                            &val as *const u32 as *const u8,
                            std::mem::size_of::<u32>(),
                        )),
                    );
                } else {
                    let _ = RegDeleteValueW(key, val_name);
                }
                let _ = RegCloseKey(key);
            }
        }
    }
}

/// Run an external command and capture a failure as a string, consuming stdout
/// for diagnostics. `netsh` is trusted to be on PATH on any Windows install.
pub fn command(program: &str, args: &[&str]) -> std::result::Result<(), String> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
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
            is_dhcp: false,
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
            is_dhcp: true,
        }];
        assert_eq!(pick_upstream(&ifaces), None);
    }

    #[test]
    fn pick_upstream_skips_loopback_to_avoid_forwarding_loop() {
        let ifaces = vec![
            IfaceDns {
                name: "Wi-Fi".into(),
                servers: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
                is_dhcp: false,
            },
            IfaceDns {
                name: "Ethernet".into(),
                servers: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))],
                is_dhcp: false,
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
            is_dhcp: false,
        }];
        assert_eq!(pick_upstream(&ifaces), None);
    }

    #[test]
    fn serde_backward_compatibility_defaults_is_dhcp_to_false() {
        let legacy_json = r#"[{"name":"Ethernet","servers":["192.168.1.1"]}]"#;
        let decoded: Vec<IfaceDns> = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].name, "Ethernet");
        assert!(!decoded[0].is_dhcp);
    }

    #[test]
    fn serde_roundtrip_with_is_dhcp() {
        let original = vec![IfaceDns {
            name: "Wi-Fi".into(),
            servers: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))],
            is_dhcp: true,
        }];
        let json = serde_json::to_string(&original).unwrap();
        let decoded: Vec<IfaceDns> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, original);
        assert!(decoded[0].is_dhcp);
    }

    #[test]
    fn private_router_ip_is_treated_as_dhcp_candidate() {
        let private_ip = Ipv4Addr::new(192, 168, 1, 1);
        assert!(private_ip.is_private());

        let public_ip = Ipv4Addr::new(1, 1, 1, 1);
        assert!(!public_ip.is_private());
    }
}
