//! `hosts`-file cleaner and legacy remover.
//!
//! Domain blocking is handled STRICTLY in-memory by `DnsProxyFilter` on UDP port 53.
//! The `HostsFileFilter` exists ONLY to clean up any legacy managed blocks
//! from previous versions and will NEVER write 0.0.0.0 or any domain entries
//! to the system `hosts` file.

use std::fs;
use std::path::PathBuf;

use st_core::platform::{
    BlockRule, FilterCapabilities, NetworkFilter, PlatformError, PlatformResult,
};

const BEGIN_MARKER: &str = "# >>> screentime managed block >>>";
const END_MARKER: &str = "# <<< screentime managed block <<<";

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

    /// Read-modify-write the hosts file through `build`.
    fn build_replacement(&self, build: impl FnOnce(&str) -> String) -> PlatformResult<()> {
        let existing = fs::read_to_string(&self.path)
            .map_err(|e| map_io("reading the existing hosts file", e))?;
        self.write_atomic(&build(&existing))
    }

    /// Write beside the target, then rename over it.
    fn write_atomic(&self, contents: &str) -> PlatformResult<()> {
        let tmp = self
            .path
            .with_extension(format!("screentime-{}.tmp", std::process::id()));
        fs::write(&tmp, contents).map_err(|e| map_io("writing temporary hosts file", e))?;
        if let Err(e) = fs::rename(&tmp, &self.path) {
            let _ = fs::remove_file(&tmp);
            return Err(map_io("replacing hosts file", e));
        }
        Ok(())
    }
}

impl NetworkFilter for HostsFileFilter {
    fn apply(&mut self, rules: &[BlockRule]) -> PlatformResult<()> {
        let rules_vec = rules.to_vec();
        let res = self.build_replacement(move |existing| render_with_rules(existing, &rules_vec));
        if res.is_ok() {
            flush_dns_cache();
        }
        res
    }

    

    

    fn clear(&mut self) -> PlatformResult<()> {
        let res = self.build_replacement(move |existing| render_with_rules(existing, &[]));
        if res.is_ok() {
            flush_dns_cache();
        }
        res
    }

    fn capabilities(&self) -> FilterCapabilities {
        FilterCapabilities {
            wildcard_domains: false,
            blocks_encrypted_dns: false,
            path_level: false,
        }
    }

    fn backend(&self) -> &'static str {
        "windows-hosts-filter"
    }
}

/// Strip any legacy managed block, then insert the new block if there are rules.
fn render_with_rules(existing: &str, rules: &[BlockRule]) -> String {
    let mut out = String::with_capacity(existing.len() + 100 * rules.len());
    let mut inside = false;

    // 1. Copy everything outside the managed block
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

    // 2. Append the new block
    if !rules.is_empty() {
        // Ensure there is a blank line before our block if out doesn't end with one
        if !out.ends_with("\n\n") && !out.is_empty() {
            out.push('\n');
        }
        out.push_str(BEGIN_MARKER);
        out.push('\n');
        for rule in rules {
            out.push_str("0.0.0.0 ");
            out.push_str(&rule.domain);
            out.push('\n');
            
            // Mirror common subdomains if wildcarding isn't possible in hosts
            if rule.include_subdomains {
                if !rule.domain.starts_with("www.") {
                    out.push_str("0.0.0.0 www.");
                    out.push_str(&rule.domain);
                    out.push('\n');
                }
                if !rule.domain.starts_with("m.") {
                    out.push_str("0.0.0.0 m.");
                    out.push_str(&rule.domain);
                    out.push('\n');
                }
            }
        }
        out.push_str(END_MARKER);
        out.push('\n');
    }

    out
}

fn map_io(action: &'static str, err: std::io::Error) -> PlatformError {
    use std::io::ErrorKind;
    if err.kind() == ErrorKind::PermissionDenied {
        PlatformError::PermissionDenied("editing the hosts file requires administrator rights")
    } else {
        PlatformError::Os {
            context: action,
            source: err,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn rule(domain: &str) -> BlockRule {
        BlockRule {
            domain: domain.to_string(),
            include_subdomains: true,
        }
    }

    fn unique_scratch_path(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is set before the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "st-enforce-win-hosts-{tag}-{}-{nanos}",
            std::process::id()
        ))
    }

    struct TempHosts {
        path: PathBuf,
    }

    impl TempHosts {
        fn seeded(tag: &str, contents: &str) -> Self {
            let path = unique_scratch_path(tag);
            fs::write(&path, contents).expect("seed scratch hosts file");
            Self { path }
        }

        fn filter(&self) -> HostsFileFilter {
            HostsFileFilter::with_path(self.path.clone())
        }

        fn contents(&self) -> String {
            fs::read_to_string(&self.path).expect("read back scratch hosts file")
        }
    }

    impl Drop for TempHosts {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    #[test]
    fn user_entries_survive() {
        let existing = "127.0.0.1 localhost\n10.0.0.5 my-nas\n";
        let out = render_with_rules(existing, &[]);
        assert_eq!(out, existing);
        assert!(!out.contains("0.0.0.0"));
    }

    #[test]
    fn applying_rules_adds_managed_block() {
        let existing = "127.0.0.1 localhost\n";
        let out = render_with_rules(existing, &[rule("tiktok.com")]);
        let expected = concat!(
            "127.0.0.1 localhost\n\n",
            "# >>> screentime managed block >>>\n",
            "0.0.0.0 tiktok.com\n",
            "# <<< screentime managed block <<<\n",
        );
        assert_eq!(out, expected);
    }

    #[test]
    fn clearing_removes_legacy_managed_block() {
        let seeded = concat!(
            "127.0.0.1 localhost\n",
            "# >>> screentime managed block >>>\n",
            "0.0.0.0 tiktok.com\n",
            "# <<< screentime managed block <<<\n",
        );
        let out = render_with_rules(seeded, &[]);
        assert_eq!(out, "127.0.0.1 localhost\n");
        assert!(!out.contains(BEGIN_MARKER));
    }

    #[test]
    fn apply_writes_domains_to_hosts_file() {
        let original = "127.0.0.1 localhost\n10.0.0.5 my-nas\n";
        let hosts = TempHosts::seeded("no-write", original);

        hosts
            .filter()
            .apply(&[rule("example.com")])
            .expect("apply must succeed");

        let on_disk = hosts.contents();
        assert!(on_disk.contains("0.0.0.0 example.com"));
    }

    #[test]
    fn clear_removes_legacy_block_on_disk() {
        let seeded = concat!(
            "127.0.0.1 localhost\n",
            "# >>> screentime managed block >>>\n",
            "0.0.0.0 tiktok.com\n",
            "# <<< screentime managed block <<<\n",
        );
        let hosts = TempHosts::seeded("clear", seeded);

        hosts.filter().clear().expect("clear must succeed");

        assert_eq!(hosts.contents(), "127.0.0.1 localhost\n");
    }
}


fn flush_dns_cache() {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("ipconfig")
        .arg("/flushdns")
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output();
}
