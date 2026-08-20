//! `hosts`-file domain blocking: the Tier 1 network filter.
//!
//! This exists because it is trivial to ship and works everywhere. It is *not*
//! sufficient on its own, and the product must not claim otherwise:
//!
//! * No wildcards. `hosts` matches exact names only, so subdomains have to be
//!   guessed and enumerated.
//! * Any browser with DNS-over-HTTPS enabled ignores it completely. Chrome and
//!   Edge enable DoH automatically on many networks, so on a default install
//!   this blocks roughly nothing in the browser that matters most.
//!
//! [`FilterCapabilities`] reports both gaps so the UI can tell the truth. The
//! Tier 2 DNS proxy in M3 is what makes filtering real.

use std::fs;
use std::path::PathBuf;

use st_core::platform::{
    BlockRule, FilterCapabilities, NetworkFilter, PlatformError, PlatformResult,
};

const BEGIN_MARKER: &str = "# >>> screentime managed block >>>";
const END_MARKER: &str = "# <<< screentime managed block <<<";

/// Prefixes tried for each blocked domain, since `hosts` cannot wildcard.
/// Covers the common cases; anything else needs the DNS proxy.
const SUBDOMAIN_PREFIXES: &[&str] = &["", "www.", "m.", "mobile.", "web."];

pub struct HostsFileFilter {
    path: PathBuf,
}

impl Default for HostsFileFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl HostsFileFilter {
    pub fn new() -> Self {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        Self {
            path: PathBuf::from(root).join("System32\\drivers\\etc\\hosts"),
        }
    }

    /// Point at an arbitrary file, for tests.
    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    fn write(&self, contents: &str) -> PlatformResult<()> {
        // Write beside the target then rename, so a crash mid-write cannot
        // leave the machine with a truncated hosts file and no name resolution.
        let tmp = self.path.with_extension("screentime-tmp");
        fs::write(&tmp, contents).map_err(|e| map_io("writing temporary hosts file", e))?;
        fs::rename(&tmp, &self.path).map_err(|e| map_io("replacing hosts file", e))?;
        Ok(())
    }
}

impl NetworkFilter for HostsFileFilter {
    fn apply(&mut self, rules: &[BlockRule]) -> PlatformResult<()> {
        let existing = fs::read_to_string(&self.path).unwrap_or_default();
        self.write(&render(&existing, rules))
    }

    fn clear(&mut self) -> PlatformResult<()> {
        let existing = fs::read_to_string(&self.path).unwrap_or_default();
        self.write(&render(&existing, &[]))
    }

    fn capabilities(&self) -> FilterCapabilities {
        FilterCapabilities {
            wildcard_domains: false,
            blocks_encrypted_dns: false,
            path_level: false,
        }
    }

    fn backend(&self) -> &'static str {
        "windows-hosts"
    }
}

/// Replace our managed block, leaving every user-authored line untouched.
///
/// Pure so it can be tested without touching the real hosts file.
fn render(existing: &str, rules: &[BlockRule]) -> String {
    let mut out = String::with_capacity(existing.len() + rules.len() * 48);
    let mut inside = false;

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed == BEGIN_MARKER {
            inside = true;
            continue;
        }
        if trimmed == END_MARKER {
            inside = false;
            continue;
        }
        if inside {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    if rules.is_empty() {
        return out;
    }

    out.push_str(BEGIN_MARKER);
    out.push('\n');
    out.push_str("# Managed automatically. Edits between the markers are overwritten.\n");
    for rule in rules {
        for host in expand(rule) {
            // 0.0.0.0 rather than 127.0.0.1: it fails immediately instead of
            // waiting for a local web server that is not listening, so blocked
            // pages error out fast rather than hanging.
            out.push_str("0.0.0.0 ");
            out.push_str(&host);
            out.push('\n');
        }
    }
    out.push_str(END_MARKER);
    out.push('\n');
    out
}

fn expand(rule: &BlockRule) -> Vec<String> {
    let domain = rule.domain.trim().trim_start_matches('.').to_lowercase();
    if domain.is_empty() {
        return Vec::new();
    }
    if !rule.include_subdomains {
        return vec![domain];
    }
    SUBDOMAIN_PREFIXES
        .iter()
        .map(|p| format!("{p}{domain}"))
        .collect()
}

fn map_io(context: &'static str, e: std::io::Error) -> PlatformError {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        PlatformError::PermissionDenied("editing the hosts file requires administrator rights")
    } else {
        PlatformError::os(context, e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(domain: &str, subdomains: bool) -> BlockRule {
        BlockRule {
            domain: domain.into(),
            include_subdomains: subdomains,
        }
    }

    #[test]
    fn user_entries_survive() {
        let existing = "127.0.0.1 localhost\n10.0.0.5 my-nas\n";
        let out = render(existing, &[rule("example.com", false)]);
        assert!(out.contains("127.0.0.1 localhost"));
        assert!(out.contains("10.0.0.5 my-nas"));
        assert!(out.contains("0.0.0.0 example.com"));
    }

    #[test]
    fn reapplying_does_not_duplicate_the_block() {
        let first = render("127.0.0.1 localhost\n", &[rule("example.com", false)]);
        let second = render(&first, &[rule("example.com", false)]);
        assert_eq!(first, second);
        assert_eq!(second.matches(BEGIN_MARKER).count(), 1);
    }

    #[test]
    fn clearing_removes_only_our_block() {
        let applied = render("127.0.0.1 localhost\n", &[rule("example.com", true)]);
        let cleared = render(&applied, &[]);
        assert_eq!(cleared, "127.0.0.1 localhost\n");
        assert!(!cleared.contains(BEGIN_MARKER));
    }

    #[test]
    fn subdomains_are_enumerated_because_hosts_cannot_wildcard() {
        let out = render("", &[rule("tiktok.com", true)]);
        assert!(out.contains("0.0.0.0 tiktok.com"));
        assert!(out.contains("0.0.0.0 www.tiktok.com"));
        assert!(out.contains("0.0.0.0 m.tiktok.com"));
    }

    #[test]
    fn domains_are_normalised() {
        let out = render("", &[rule("  .Example.COM ", false)]);
        assert!(out.contains("0.0.0.0 example.com"));
        assert!(!out.contains("Example"));
    }

    #[test]
    fn empty_domains_are_skipped() {
        let out = render("", &[rule("   ", true)]);
        assert!(!out.contains("0.0.0.0 \n"));
    }

    #[test]
    fn capabilities_admit_the_doh_hole() {
        let f = HostsFileFilter::with_path(PathBuf::from("hosts"));
        let caps = f.capabilities();
        assert!(!caps.blocks_encrypted_dns, "must not overstate enforcement");
        assert!(!caps.wildcard_domains);
        assert!(!caps.path_level);
    }
}
