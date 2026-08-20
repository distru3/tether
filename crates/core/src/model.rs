//! Core entities: how an application is identified, and what a slice of usage
//! looks like.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::daykey::DayKey;

pub type AppId = i64;
pub type SiteId = i64;
pub type CategoryId = i64;

/// Stable identity for an application across restarts and updates.
///
/// Getting this right matters more than it looks: if the key changes when an app
/// updates, the user's limits silently stop applying. Hence path-based keys are
/// normalised (lower-cased on Windows, which is case-insensitive) and packaged
/// apps prefer their publisher-assigned identifier over any file path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AppKey {
    /// Classic Windows executable, keyed by normalised full path.
    WindowsExe(String),
    /// Windows packaged app (UWP/MSIX/Store), keyed by Application User Model ID.
    ///
    /// Preferred over the path because the foreground window of a packaged app
    /// belongs to `ApplicationFrameHost.exe`, and because paths contain a
    /// version number that changes on every update.
    WindowsAumid(String),
    /// Linux freedesktop desktop-entry id, e.g. `org.mozilla.firefox`.
    LinuxDesktop(String),
    /// Linux executable path, used when no desktop entry can be matched.
    LinuxExe(String),
    /// Flatpak or Snap application id. Sandboxed apps all look like `bwrap`
    /// at the process level, so the sandbox id is the only useful identity.
    Sandboxed(String),
}

impl AppKey {
    /// Canonical `kind:value` form used as the database primary key.
    pub fn to_db_string(&self) -> String {
        let (kind, value) = match self {
            AppKey::WindowsExe(v) => ("win-exe", v),
            AppKey::WindowsAumid(v) => ("win-aumid", v),
            AppKey::LinuxDesktop(v) => ("linux-desktop", v),
            AppKey::LinuxExe(v) => ("linux-exe", v),
            AppKey::Sandboxed(v) => ("sandboxed", v),
        };
        format!("{kind}:{value}")
    }

    pub fn parse_db_string(s: &str) -> Option<Self> {
        let (kind, value) = s.split_once(':')?;
        let value = value.to_string();
        Some(match kind {
            "win-exe" => AppKey::WindowsExe(value),
            "win-aumid" => AppKey::WindowsAumid(value),
            "linux-desktop" => AppKey::LinuxDesktop(value),
            "linux-exe" => AppKey::LinuxExe(value),
            "sandboxed" => AppKey::Sandboxed(value),
            _ => return None,
        })
    }

    /// Build a Windows executable key with the normalisation the platform needs.
    pub fn windows_exe(path: &str) -> Self {
        AppKey::WindowsExe(path.replace('/', "\\").to_lowercase())
    }

    /// Last path component, used for signature-database lookups and as a
    /// display-name fallback.
    pub fn basename(&self) -> &str {
        match self {
            AppKey::WindowsExe(v) | AppKey::LinuxExe(v) => v
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or(v.as_str()),
            AppKey::WindowsAumid(v) | AppKey::LinuxDesktop(v) | AppKey::Sandboxed(v) => v.as_str(),
        }
    }
}

impl fmt::Display for AppKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_db_string())
    }
}

/// An application known to the system.
///
/// `primary_category` is what reports and charts attribute time to, so category
/// totals always sum to 100%. `tags` are additional categories used *only* for
/// limit matching, which is how TikTok can be both Social Media and Short-Form
/// Video without being double-counted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppRecord {
    pub id: AppId,
    pub key: AppKey,
    pub display_name: String,
    pub publisher: Option<String>,
    pub primary_category: CategoryId,
    pub tags: Vec<CategoryId>,
    /// True once the user has explicitly categorised this app; classifier
    /// updates must never overwrite a human decision.
    pub user_classified: bool,
}

impl AppRecord {
    /// Every category this app belongs to, for limit evaluation.
    pub fn all_categories(&self) -> Vec<CategoryId> {
        let mut out = Vec::with_capacity(self.tags.len() + 1);
        out.push(self.primary_category);
        for t in &self.tags {
            if *t != self.primary_category {
                out.push(*t);
            }
        }
        out
    }
}

/// A website known to the system, keyed by registrable domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteRecord {
    pub id: SiteId,
    pub domain: String,
    pub primary_category: CategoryId,
    pub tags: Vec<CategoryId>,
    pub user_classified: bool,
}

/// What a usage interval is attributed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubjectRef {
    App(AppId),
    Site(SiteId),
}

/// A contiguous span of foreground usage.
///
/// The sampler polls at ~1 Hz but collapses identical consecutive samples into
/// intervals, so a two-hour session is one row rather than 7200.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageInterval {
    pub subject: SubjectRef,
    /// Identifies the login session, so two logged-in users are accounted
    /// separately.
    pub session_id: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub day_key: DayKey,
}

impl UsageInterval {
    pub fn duration_secs(&self) -> i64 {
        (self.end - self.start).num_seconds().max(0)
    }
}
