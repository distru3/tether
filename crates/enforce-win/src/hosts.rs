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

    /// Read-modify-write the hosts file through `build`.
    ///
    /// Why a read failure aborts instead of falling back to an empty string:
    /// an unreadable hosts file means we do not know what it currently
    /// contains, and writing anyway would replace every user entry (custom
    /// routes, printer mappings, existing blocklists) with just our managed
    /// block — silent, unrecoverable data loss on a system file. Surfacing the
    /// error leaves the user's file byte-for-byte intact and the operation can
    /// be retried once whatever holds the file (an AV scanner, a sync client)
    /// lets go. A missing file is likewise an error, never silently recreated.
    fn build_replacement(&self, build: impl FnOnce(&str) -> String) -> PlatformResult<()> {
        let existing = fs::read_to_string(&self.path)
            .map_err(|e| map_io("reading the existing hosts file", e))?;
        self.write_atomic(&build(&existing))
    }

    /// Write beside the target, then rename over it.
    ///
    /// Why: truncating the hosts file in place means a crash mid-write leaves
    /// the machine with no usable name resolution at all — worse than the bug
    /// being fixed. A same-volume rename is atomic on NTFS, so readers see
    /// either the old or the new file, never a half-written one.
    ///
    /// # Residual risks on Windows (documented, accepted for M0)
    ///
    /// * `fs::rename` uses `MOVEFILE_REPLACE_EXISTING`, which fails if another
    ///   process holds the target open without share-delete (antivirus,
    ///   indexers). That surfaces as an error with the original untouched and
    ///   the temp file removed; the caller can simply retry.
    /// * The replacement inherits default ACLs/attributes from the `etc`
    ///   directory rather than copying the original's exactly.
    /// * A crash between write and rename can strand the temp file; it is
    ///   inert (nothing reads it) and the pid-suffixed name is overwritten by
    ///   the next attempt.
    fn write_atomic(&self, contents: &str) -> PlatformResult<()> {
        // Pid suffix so two agent instances cannot clobber each other's temp.
        let tmp = self
            .path
            .with_extension(format!("screentime-{}.tmp", std::process::id()));
        fs::write(&tmp, contents).map_err(|e| map_io("writing temporary hosts file", e))?;
        if let Err(e) = fs::rename(&tmp, &self.path) {
            // Leave no litter behind; the original hosts file is untouched.
            let _ = fs::remove_file(&tmp);
            return Err(map_io("replacing hosts file", e));
        }
        Ok(())
    }
}

impl NetworkFilter for HostsFileFilter {
    fn apply(&mut self, rules: &[BlockRule]) -> PlatformResult<()> {
        self.build_replacement(|existing| render(existing, rules))
    }

    fn clear(&mut self) -> PlatformResult<()> {
        self.build_replacement(|existing| render(existing, &[]))
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
    use std::os::windows::fs::OpenOptionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn rule(domain: &str, subdomains: bool) -> BlockRule {
        BlockRule {
            domain: domain.into(),
            include_subdomains: subdomains,
        }
    }

    /// A process+time-unique path under the system temp dir, so parallel test
    /// runs and leftovers from earlier runs cannot collide. Std-only on
    /// purpose: a `tempfile` dev-dependency buys nothing over this.
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

    /// A seeded scratch hosts file that removes itself when the test ends.
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
            // Best effort only; a leaked scratch file in TEMP is harmless.
            let _ = fs::remove_file(&self.path);
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

    // --- Real file-I/O tests -------------------------------------------------
    //
    // The pure `render` tests above prove the string transformation; these
    // prove the read-modify-write around it: bytes actually land on disk,
    // failures leave the original untouched.

    #[test]
    fn apply_preserves_pre_existing_user_lines_verbatim_on_disk() {
        let original = "127.0.0.1 localhost\n10.0.0.5 my-nas\n";
        let hosts = TempHosts::seeded("preserve", original);

        hosts
            .filter()
            .apply(&[rule("Example.COM", false)])
            .expect("apply must succeed on a readable file");

        let on_disk = hosts.contents();
        assert!(
            on_disk.starts_with(original),
            "user lines must survive byte-for-byte at the top, got:\n{on_disk}"
        );
        assert!(on_disk.contains("0.0.0.0 example.com"));
    }

    #[test]
    fn clear_removes_only_marker_delimited_lines_on_disk() {
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

    #[test]
    fn repeated_applies_round_trip_idempotently_on_disk() {
        let hosts = TempHosts::seeded("idempotent", "127.0.0.1 localhost\n");
        let mut filter = hosts.filter();

        filter
            .apply(&[rule("example.com", false)])
            .expect("first apply");
        let once = hosts.contents();
        filter
            .apply(&[rule("example.com", false)])
            .expect("second apply");

        assert_eq!(once, hosts.contents());
        assert_eq!(once.matches(BEGIN_MARKER).count(), 1);
        assert!(once.starts_with("127.0.0.1 localhost\n"));

        // A changed rule set replaces the block in place, still idempotently.
        filter
            .apply(&[rule("example.com", true)])
            .expect("third apply");
        let expanded = hosts.contents();
        filter
            .apply(&[rule("example.com", true)])
            .expect("fourth apply");

        assert_eq!(expanded, hosts.contents());
        assert!(expanded.contains("0.0.0.0 www.example.com"));
    }

    #[test]
    fn unreadable_hosts_file_aborts_leaving_original_bytes_untouched() {
        let original = "127.0.0.1 localhost\n192.168.1.20 build-box\n";
        let hosts = TempHosts::seeded("locked", original);

        // Hold an exclusive (share_mode 0) handle so every other opener gets a
        // sharing violation — the realistic "AV scanner or sync client is
        // holding the file" failure mode the audit was about.
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&hosts.path)
            .expect("take exclusive lock on scratch file");

        let err = hosts
            .filter()
            .apply(&[rule("evil.example", false)])
            .expect_err("a failed read must abort the apply, never fall back to empty");
        assert!(!matches!(err, PlatformError::Unsupported(_)));

        drop(lock);
        assert_eq!(
            hosts.contents(),
            original,
            "a failed apply must not modify a single byte of the file"
        );
    }

    #[test]
    fn directory_as_hosts_path_aborts_apply_and_clear() {
        // The directory-as-file trick: reading a path that is really a
        // directory always fails, simulating an unreadable target without any
        // platform-specific locking.
        let dir = unique_scratch_path("dir");
        fs::create_dir_all(&dir).expect("create scratch directory");

        let mut filter = HostsFileFilter::with_path(dir.clone());
        let applied = filter.apply(&[rule("example.com", false)]);
        let cleared = filter.clear();
        let _ = fs::remove_dir(&dir);

        assert!(
            applied.is_err(),
            "must abort, not write a managed-only file"
        );
        assert!(cleared.is_err());
    }

    #[test]
    fn missing_hosts_file_is_an_error_not_silently_recreated() {
        let path = unique_scratch_path("missing");
        let mut filter = HostsFileFilter::with_path(path.clone());

        let result = filter.apply(&[rule("example.com", false)]);

        assert!(result.is_err());
        assert!(
            !path.exists(),
            "apply must not fabricate a hosts file from nothing"
        );
    }
}
