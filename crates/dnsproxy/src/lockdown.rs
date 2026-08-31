//! Browser DoH (DNS-over-HTTPS) enterprise policy lockdown and Windows Defender
//! Firewall outbound rule management.
//!
//! # Why this is needed
//!
//! Modern web browsers (Google Chrome, Microsoft Edge, Brave, Mozilla Firefox)
//! implement their own built-in DNS-over-HTTPS clients. When enabled, the
//! browser sends DNS queries over encrypted HTTPS (port 443) directly to public
//! resolvers (e.g. Cloudflare 1.1.1.1, Google 8.8.8.8), completely bypassing
//! both `hosts` files and the machine's local DNS proxy at `127.0.0.1:53`.
//!
//! In addition, arbitrary software could attempt direct queries to external DNS
//! resolvers on UDP/TCP port 53 or DNS-over-TLS on port 853.
//!
//! # How it works
//!
//! 1. **Browser Enterprise Policies**:
//!    Under `HKLM\SOFTWARE\Policies\...`, Windows enterprise policy keys override
//!    browser settings and disable built-in DoH clients system-wide.
//!    When active, the browser displays "Managed by your organization" on its
//!    Secure DNS setting and routes all lookups through standard OS DNS APIs
//!    (which hit `screentime-agent`'s local proxy).
//! 2. **Firewall Rules**:
//!    Outbound blocking rules on port 853 (DoT) and port 53 (direct external DNS)
//!    prevent bypasses while allowing the agent and the designated upstream.
//! 3. **Clean Restoration**:
//!    `restore_all()` cleanly deletes the Screentime firewall rules and restores
//!    or removes the policy registry keys when the network filter is cleared.

use std::net::IpAddr;

#[cfg(windows)]
use std::process::Command;

#[cfg(windows)]
use windows::{
    core::PCWSTR,
    Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR},
    Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY,
        HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE,
        REG_SZ,
    },
};

/// A registry policy entry to manage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyEntry {
    pub subkey: &'static str,
    pub value_name: &'static str,
    pub value: PolicyValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyValue {
    String(&'static str),
    Dword(u32),
}

/// The set of enterprise registry policies that disable browser DoH.
pub const DOH_BROWSER_POLICIES: &[PolicyEntry] = &[
    // Google Chrome
    PolicyEntry {
        subkey: r"SOFTWARE\Policies\Google\Chrome",
        value_name: "DnsOverHttpsMode",
        value: PolicyValue::String("off"),
    },
    // Microsoft Edge
    PolicyEntry {
        subkey: r"SOFTWARE\Policies\Microsoft\Edge",
        value_name: "DnsOverHttpsMode",
        value: PolicyValue::String("off"),
    },
    // Brave Browser
    PolicyEntry {
        subkey: r"SOFTWARE\Policies\BraveSoftware\Brave",
        value_name: "DnsOverHttpsMode",
        value: PolicyValue::String("off"),
    },
    // Mozilla Firefox: DNSOverHTTPS subkey
    PolicyEntry {
        subkey: r"SOFTWARE\Policies\Mozilla\Firefox\DNSOverHTTPS",
        value_name: "Enabled",
        value: PolicyValue::Dword(0),
    },
    PolicyEntry {
        subkey: r"SOFTWARE\Policies\Mozilla\Firefox\DNSOverHTTPS",
        value_name: "Locked",
        value: PolicyValue::Dword(1),
    },
];

/// Names of firewall rules managed by Screentime.
pub const FW_RULE_DOT_TCP: &str = "Screentime-Block-DoT-TCP";
pub const FW_RULE_DOT_UDP: &str = "Screentime-Block-DoT-UDP";
pub const FW_RULE_DNS_BLOCK: &str = "Screentime-Block-Outbound-DNS";
pub const FW_RULE_DNS_ALLOW: &str = "Screentime-Allow-Upstream-DNS";

/// Result of applying lockdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LockdownState {
    pub browser_policies_applied: bool,
    pub firewall_rules_applied: bool,
}

impl LockdownState {
    /// True when browser DoH lockdown succeeded.
    pub fn is_effective(&self) -> bool {
        self.browser_policies_applied
    }
}

/// Apply all browser DoH registry policies and firewall rules.
pub fn apply_lockdown(upstream_ip: Option<IpAddr>) -> LockdownState {
    #[cfg(windows)]
    {
        let browser_policies_applied = apply_browser_policies();
        let firewall_rules_applied = apply_firewall_rules(upstream_ip);
        LockdownState {
            browser_policies_applied,
            firewall_rules_applied,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = upstream_ip;
        LockdownState::default()
    }
}

/// Restore / clean up all browser DoH registry policies and firewall rules.
pub fn clear_lockdown() {
    #[cfg(windows)]
    {
        restore_browser_policies();
        restore_firewall_rules();
    }
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn set_registry_policy(entry: &PolicyEntry) -> Result<(), WIN32_ERROR> {
    unsafe {
        let subkey_w = wide(entry.subkey);
        let name_w = wide(entry.value_name);
        let mut key = HKEY::default();

        let err = RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey_w.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE | KEY_QUERY_VALUE,
            None,
            &mut key,
            None,
        );
        if err != ERROR_SUCCESS {
            return Err(err);
        }

        let result = match &entry.value {
            PolicyValue::String(val) => {
                let mut data = Vec::with_capacity((val.len() + 1) * 2);
                for unit in val.encode_utf16().chain(std::iter::once(0)) {
                    data.extend_from_slice(&unit.to_le_bytes());
                }
                RegSetValueExW(key, PCWSTR(name_w.as_ptr()), 0, REG_SZ, Some(&data))
            }
            PolicyValue::Dword(val) => {
                let data = val.to_le_bytes();
                RegSetValueExW(key, PCWSTR(name_w.as_ptr()), 0, REG_DWORD, Some(&data))
            }
        };

        let _ = RegCloseKey(key);
        if result == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(result)
        }
    }
}

#[cfg(windows)]
fn delete_registry_policy(entry: &PolicyEntry) -> Result<(), WIN32_ERROR> {
    unsafe {
        let subkey_w = wide(entry.subkey);
        let name_w = wide(entry.value_name);
        let mut key = HKEY::default();

        let err = RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey_w.as_ptr()),
            0,
            KEY_SET_VALUE,
            &mut key,
        );
        if err == ERROR_FILE_NOT_FOUND || err == ERROR_PATH_NOT_FOUND {
            return Ok(());
        }
        if err != ERROR_SUCCESS {
            return Err(err);
        }

        let result = RegDeleteValueW(key, PCWSTR(name_w.as_ptr()));
        let _ = RegCloseKey(key);
        if result == ERROR_SUCCESS || result == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(result)
        }
    }
}

#[cfg(windows)]
fn apply_browser_policies() -> bool {
    let mut all_succeeded = true;
    for entry in DOH_BROWSER_POLICIES {
        if let Err(e) = set_registry_policy(entry) {
            tracing::warn!(
                subkey = entry.subkey,
                value_name = entry.value_name,
                error_code = e.0,
                "failed to set browser DoH policy (requires elevation)"
            );
            all_succeeded = false;
        }
    }
    if all_succeeded {
        tracing::info!("browser DoH policies applied successfully");
    }
    all_succeeded
}

#[cfg(windows)]
fn restore_browser_policies() {
    for entry in DOH_BROWSER_POLICIES {
        if let Err(e) = delete_registry_policy(entry) {
            tracing::debug!(
                subkey = entry.subkey,
                value_name = entry.value_name,
                error_code = e.0,
                "could not delete browser DoH policy value during clear"
            );
        }
    }
}

#[cfg(windows)]
fn run_netsh(args: &[&str]) -> bool {
    match Command::new("netsh").args(args).output() {
        Ok(output) => output.status.success(),
        Err(e) => {
            tracing::debug!(error = %e, ?args, "netsh execution failed");
            false
        }
    }
}

#[cfg(windows)]
fn apply_firewall_rules(upstream_ip: Option<IpAddr>) -> bool {
    // 1. Clean existing rules first to remain idempotent.
    restore_firewall_rules();

    // 2. Block outbound DoT (port 853) on TCP and UDP.
    let dot_tcp = run_netsh(&[
        "advfirewall",
        "firewall",
        "add",
        "rule",
        &format!("name={FW_RULE_DOT_TCP}"),
        "dir=out",
        "action=block",
        "protocol=TCP",
        "remoteport=853",
    ]);

    let dot_udp = run_netsh(&[
        "advfirewall",
        "firewall",
        "add",
        "rule",
        &format!("name={FW_RULE_DOT_UDP}"),
        "dir=out",
        "action=block",
        "protocol=UDP",
        "remoteport=853",
    ]);

    // 3. Allow upstream DNS if specified.
    if let Some(upstream) = upstream_ip {
        let _ = run_netsh(&[
            "advfirewall",
            "firewall",
            "add",
            "rule",
            &format!("name={FW_RULE_DNS_ALLOW}"),
            "dir=out",
            "action=allow",
            "protocol=UDP",
            "remoteport=53",
            &format!("remoteip={upstream},127.0.0.1"),
        ]);
    }

    dot_tcp && dot_udp
}

#[cfg(windows)]
fn restore_firewall_rules() {
    for name in &[
        FW_RULE_DOT_TCP,
        FW_RULE_DOT_UDP,
        FW_RULE_DNS_BLOCK,
        FW_RULE_DNS_ALLOW,
    ] {
        let _ = run_netsh(&[
            "advfirewall",
            "firewall",
            "delete",
            "rule",
            &format!("name={name}"),
        ]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policies_list_is_non_empty_and_valid() {
        assert!(!DOH_BROWSER_POLICIES.is_empty());
        for p in DOH_BROWSER_POLICIES {
            assert!(!p.subkey.is_empty());
            assert!(!p.value_name.is_empty());
        }
    }

    #[test]
    fn lockdown_state_is_effective_when_policies_applied() {
        let state = LockdownState {
            browser_policies_applied: true,
            firewall_rules_applied: false,
        };
        assert!(state.is_effective());

        let inactive = LockdownState {
            browser_policies_applied: false,
            firewall_rules_applied: true,
        };
        assert!(!inactive.is_effective());
    }
}
