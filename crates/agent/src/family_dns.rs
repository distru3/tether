//! Family DNS: point every active adapter at Cloudflare Family DNS
//! (`1.1.1.3` / `1.0.0.3`) and put it back exactly as it was afterwards.
//!
//! # One implementation
//!
//! This used to exist three times — inline in the `SetSetting` IPC handler and
//! in the `--enable-family-dns` / `--disable-family-dns` CLI branches — with
//! different behaviour: only the CLI applied the browser DoH lockdown, only
//! the IPC path kept a database copy of the backup, and the uninstaller's
//! `--disable-family-dns` ignored that copy. All three now call [`enable`] /
//! [`disable`].
//!
//! # Ordering rules
//!
//! * **Never override what was not first captured.** Capture failure aborts
//!   before anything is changed.
//! * **Never recapture over an existing backup.** If a backup survives (a
//!   crash between override and flag write), it is reused: recapturing would
//!   record Family DNS itself as the "original" and make restore a no-op.
//! * **No database lock across OS calls.** `netsh` can take seconds; holding
//!   the lock would stall the 1 Hz enforcement loop and every IPC client.
//! * **Errors surface.** A failed apply is rolled back and reported to the
//!   caller instead of leaving the toggle claiming protection that is absent.
//!
//! The backup lives in two places on purpose: the `original_dns_config`
//! setting (survives a deleted data file) and `{data_dir}/original_dns_backup.json`
//! (survives a corrupted database, and is what an offline uninstaller finds).

use std::path::Path;
use std::sync::Mutex;

use st_core::settings::SettingKey;
use st_storage::Db;

use crate::locks::lock_db;

/// File copy of the captured adapter configuration.
pub(crate) const BACKUP_FILE: &str = "original_dns_backup.json";
/// Settings-table copy of the captured adapter configuration.
const BACKUP_SETTING: &str = "original_dns_config";

/// The OS side, abstracted so the orchestration is testable.
///
/// Captured configurations travel as opaque JSON strings: only the backend
/// knows their shape.
pub(crate) trait DnsBackend: Send + Sync {
    /// Snapshot the current adapter DNS configuration.
    fn capture(&self) -> Result<String, String>;
    /// Point the captured adapters at Family DNS and apply the DoH lockdown.
    fn apply(&self, captured: &str) -> Result<(), String>;
    /// Undo [`DnsBackend::apply`]: restore `captured` when available,
    /// otherwise return every active adapter to DHCP. Clears the lockdown.
    fn restore(&self, captured: Option<&str>) -> Result<(), String>;
}

/// Turn Family DNS on. Idempotent.
pub(crate) fn enable(
    db: &Mutex<Db>,
    backend: &dyn DnsBackend,
    data_dir: Option<&Path>,
) -> Result<(), String> {
    let (already, stored) = {
        let db = lock_db(db);
        let flag = db
            .setting(SettingKey::FamilyDns.storage_key())
            .map_err(|e| format!("read family DNS flag: {e}"))?;
        let stored = db
            .setting(BACKUP_SETTING)
            .map_err(|e| format!("read DNS backup: {e}"))?;
        (flag.as_deref() == Some("true"), stored)
    };
    if already {
        return Ok(());
    }

    let captured = match stored.or_else(|| read_backup_file(data_dir)) {
        Some(existing) => {
            tracing::warn!("reusing a surviving DNS backup instead of recapturing");
            existing
        }
        None => backend.capture()?,
    };

    lock_db(db)
        .set_setting(BACKUP_SETTING, &captured)
        .map_err(|e| format!("store DNS backup: {e}"))?;
    write_backup_file(data_dir, &captured);

    if let Err(e) = backend.apply(&captured) {
        tracing::error!(error = %e, "applying Family DNS failed; rolling back");
        if let Err(undo) = backend.restore(Some(&captured)) {
            tracing::error!(error = %undo, "rollback after failed Family DNS apply also failed");
        }
        return Err(e);
    }

    lock_db(db)
        .set_setting(SettingKey::FamilyDns.storage_key(), "true")
        .map_err(|e| format!("store family DNS flag: {e}"))?;
    tracing::info!("Family DNS enabled");
    Ok(())
}

/// Turn Family DNS off and restore the captured configuration. `db` is `None`
/// when no database is reachable (e.g. an uninstaller running against a
/// half-removed install); the file backup is used alone then.
pub(crate) fn disable(
    db: Option<&Mutex<Db>>,
    backend: &dyn DnsBackend,
    data_dir: Option<&Path>,
) -> Result<(), String> {
    let stored = match db {
        Some(db) => lock_db(db)
            .setting(BACKUP_SETTING)
            .map_err(|e| format!("read DNS backup: {e}"))?,
        None => None,
    };
    let backup = stored.or_else(|| read_backup_file(data_dir));
    backend.restore(backup.as_deref())?;

    if let Some(db) = db {
        let db = lock_db(db);
        db.set_setting(SettingKey::FamilyDns.storage_key(), "false")
            .map_err(|e| format!("store family DNS flag: {e}"))?;
        db.conn()
            .execute("DELETE FROM settings WHERE key = ?1", [BACKUP_SETTING])
            .map_err(|e| format!("delete DNS backup: {e}"))?;
    }
    if let Some(dir) = data_dir {
        let _ = std::fs::remove_file(dir.join(BACKUP_FILE));
    }
    tracing::info!(
        restored_from_backup = backup.is_some(),
        "Family DNS disabled"
    );
    Ok(())
}

fn read_backup_file(data_dir: Option<&Path>) -> Option<String> {
    let content = std::fs::read_to_string(data_dir?.join(BACKUP_FILE)).ok()?;
    (!content.trim().is_empty()).then_some(content)
}

fn write_backup_file(data_dir: Option<&Path>, captured: &str) {
    let Some(dir) = data_dir else { return };
    if let Err(e) =
        std::fs::create_dir_all(dir).and_then(|()| std::fs::write(dir.join(BACKUP_FILE), captured))
    {
        // The database copy is authoritative; the file is a second chance.
        tracing::warn!(error = %e, "could not write DNS backup file");
    }
}

/// The real backend: `st_dns` adapter configuration plus the browser DoH /
/// DoT lockdown.
#[cfg(windows)]
pub(crate) struct WindowsDns;

#[cfg(windows)]
impl WindowsDns {
    const FAMILY_PRIMARY: std::net::Ipv4Addr = std::net::Ipv4Addr::new(1, 1, 1, 3);

    fn parse(captured: &str) -> Result<Vec<st_dns::dns_config::IfaceDns>, String> {
        serde_json::from_str(captured).map_err(|e| format!("unreadable DNS backup: {e}"))
    }

    /// Return every currently active adapter to DHCP-assigned DNS. The
    /// fallback when no backup exists, and the core of `--reset-network`.
    pub(crate) fn reset_all_to_dhcp() -> Result<(), String> {
        let ifaces = st_dns::dns_config::capture().map_err(|e| e.to_string())?;
        for iface in &ifaces {
            for family in ["ipv4", "ipv6"] {
                let _ = st_dns::dns_config::command(
                    "netsh",
                    &[
                        "interface",
                        family,
                        "set",
                        "dnsservers",
                        &format!("name={}", iface.name),
                        "source=dhcp",
                    ],
                );
            }
        }
        let _ = st_dns::dns_config::command("ipconfig", &["/flushdns"]);
        Ok(())
    }
}

#[cfg(windows)]
impl DnsBackend for WindowsDns {
    fn capture(&self) -> Result<String, String> {
        let ifaces = st_dns::dns_config::capture().map_err(|e| e.to_string())?;
        if !st_dns::dns_config::has_captured(&ifaces) {
            return Err("no active network adapters to configure".into());
        }
        serde_json::to_string(&ifaces).map_err(|e| e.to_string())
    }

    fn apply(&self, captured: &str) -> Result<(), String> {
        let ifaces = Self::parse(captured)?;
        st_dns::dns_config::set_family_dns(&ifaces);
        let lockdown = st_dns::lockdown::apply_lockdown(Some(Self::FAMILY_PRIMARY.into()));
        if !lockdown.is_effective() {
            // DNS itself is switched; only the browser DoH policy failed. Keep
            // going but say so: browsers with DoH on can still bypass.
            tracing::warn!("browser DoH lockdown not applied; DoH-enabled browsers may bypass");
        }
        st_dns::dns_config::set_registry_family_dns(true);
        Ok(())
    }

    fn restore(&self, captured: Option<&str>) -> Result<(), String> {
        st_dns::lockdown::clear_lockdown();
        let restored = match captured.map(Self::parse) {
            Some(Ok(ifaces)) => {
                st_dns::dns_config::restore_all(&ifaces);
                Ok(())
            }
            Some(Err(e)) => {
                tracing::warn!(error = %e, "DNS backup unreadable; falling back to DHCP");
                Self::reset_all_to_dhcp()
            }
            None => Self::reset_all_to_dhcp(),
        };
        st_dns::dns_config::set_registry_family_dns(false);
        restored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// Records calls; `capture`/`apply` results are scripted.
    #[derive(Default)]
    struct FakeDns {
        capture_result: Option<Result<String, String>>,
        fail_apply: bool,
        calls: StdMutex<Vec<String>>,
    }

    impl FakeDns {
        fn capturing(json: &str) -> Self {
            Self {
                capture_result: Some(Ok(json.into())),
                ..Self::default()
            }
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("calls").clone()
        }
    }

    impl DnsBackend for FakeDns {
        fn capture(&self) -> Result<String, String> {
            self.calls.lock().expect("calls").push("capture".into());
            self.capture_result
                .clone()
                .unwrap_or_else(|| Err("no capture scripted".into()))
        }
        fn apply(&self, captured: &str) -> Result<(), String> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("apply {captured}"));
            if self.fail_apply {
                Err("netsh failed".into())
            } else {
                Ok(())
            }
        }
        fn restore(&self, captured: Option<&str>) -> Result<(), String> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("restore {}", captured.unwrap_or("<dhcp>")));
            Ok(())
        }
    }

    fn db() -> Mutex<Db> {
        Mutex::new(Db::open_in_memory().expect("db"))
    }

    fn flag(db: &Mutex<Db>) -> Option<String> {
        lock_db(db).setting("family_dns_enabled").expect("read")
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("st-family-dns-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn enable_captures_backs_up_twice_then_applies() {
        let db = db();
        let dir = temp_dir("enable");
        let dns = FakeDns::capturing("[orig]");
        enable(&db, &dns, Some(&dir)).expect("enable");

        assert_eq!(dns.calls(), ["capture", "apply [orig]"]);
        assert_eq!(flag(&db).as_deref(), Some("true"));
        assert_eq!(
            lock_db(&db)
                .setting(BACKUP_SETTING)
                .expect("read")
                .as_deref(),
            Some("[orig]")
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(BACKUP_FILE)).expect("file"),
            "[orig]"
        );
    }

    #[test]
    fn enable_is_idempotent() {
        let db = db();
        let dns = FakeDns::capturing("[orig]");
        enable(&db, &dns, None).expect("first");
        enable(&db, &dns, None).expect("second");
        assert_eq!(dns.calls().len(), 2, "second enable touches nothing");
    }

    #[test]
    fn enable_never_recaptures_over_a_surviving_backup() {
        let db = db();
        lock_db(&db)
            .set_setting(BACKUP_SETTING, "[pre-crash original]")
            .expect("seed");
        let dns = FakeDns::capturing("[already family]");
        enable(&db, &dns, None).expect("enable");
        assert_eq!(dns.calls(), ["apply [pre-crash original]"]);
    }

    #[test]
    fn capture_failure_changes_nothing() {
        let db = db();
        let dns = FakeDns {
            capture_result: Some(Err("no adapters".into())),
            ..FakeDns::default()
        };
        assert!(enable(&db, &dns, None).is_err());
        assert_eq!(dns.calls(), ["capture"]);
        assert_eq!(flag(&db), None);
    }

    #[test]
    fn a_failed_apply_rolls_back_and_reports() {
        let db = db();
        let dns = FakeDns {
            capture_result: Some(Ok("[orig]".into())),
            fail_apply: true,
            ..FakeDns::default()
        };
        assert_eq!(enable(&db, &dns, None), Err("netsh failed".into()));
        assert_eq!(dns.calls(), ["capture", "apply [orig]", "restore [orig]"]);
        assert_ne!(flag(&db).as_deref(), Some("true"));
    }

    #[test]
    fn disable_restores_the_backup_and_clears_both_copies() {
        let db = db();
        let dir = temp_dir("disable");
        let dns = FakeDns::capturing("[orig]");
        enable(&db, &dns, Some(&dir)).expect("enable");
        disable(Some(&db), &dns, Some(&dir)).expect("disable");

        assert_eq!(
            dns.calls().last().map(String::as_str),
            Some("restore [orig]")
        );
        assert_eq!(flag(&db).as_deref(), Some("false"));
        assert_eq!(lock_db(&db).setting(BACKUP_SETTING).expect("read"), None);
        assert!(!dir.join(BACKUP_FILE).exists());
    }

    #[test]
    fn disable_without_a_database_uses_the_file_backup() {
        let dir = temp_dir("offline");
        std::fs::write(dir.join(BACKUP_FILE), "[from file]").expect("seed");
        let dns = FakeDns::default();
        disable(None, &dns, Some(&dir)).expect("disable");
        assert_eq!(dns.calls(), ["restore [from file]"]);
    }

    #[test]
    fn disable_with_no_backup_falls_back_to_dhcp() {
        let db = db();
        let dns = FakeDns::default();
        disable(Some(&db), &dns, None).expect("disable");
        assert_eq!(dns.calls(), ["restore <dhcp>"]);
    }
}
