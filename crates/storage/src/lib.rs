//! SQLite persistence for the screentime agent.
//!
//! The database lives in a directory owned by SYSTEM/root with restrictive
//! ACLs. The UI never opens it directly; it asks the agent over IPC. That is
//! what stops "just edit the database" from being the easiest bypass.
//!
//! # Module map
//!
//! The schema's natural seams are also the code's: every module below adds
//! methods to the same [`Db`] handle, so callers keep a single connection and
//! a single mutex around it.
//!
//! * [`migrations`] — ordered schema upgrades behind `PRAGMA user_version`.
//! * [`taxonomy`] — categories and apps (classification of what is running).
//! * [`usage`] — raw foreground intervals and the daily rollup they fold into.
//! * [`limits`] — limits plus the anti-impulse `pending_limits` queue.
//! * [`enforcement`] — block state and PIN-approved override extensions.
//! * [`settings`] — key/value settings, the PIN hash, and the audit log.
//! * [`reporting`] — dashboard summaries and limits-engine day snapshots.

mod enforcement;
mod limits;
mod migrations;
mod reporting;
mod settings;
mod taxonomy;
mod usage;

pub use crate::limits::LimitRow;
pub use crate::reporting::{
    CategoryRow, DailyTotal, DaySnapshot, DaySummary, UsageRow, WeeklySummary,
};

use std::path::Path;

use rusqlite::Connection;
use st_core::limits::LimitTarget;
use st_core::model::SubjectRef;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("malformed json in column {column}: {source}")]
    Json {
        column: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("unknown category slug: {0}")]
    UnknownCategory(String),
    #[error("app not found: {0}")]
    AppNotFound(i64),
    #[error("invalid day key: {0}")]
    InvalidDay(i32),
}

/// The agent's single SQLite handle. All queries go through it so that WAL
/// mode, the busy timeout, and migrations are applied exactly once, in one
/// place ([`Db::bootstrap`]).
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open (creating if needed) and bring the schema up to date.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::bootstrap(conn)
    }

    /// In-memory database, for tests.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::bootstrap(conn)
    }

    fn bootstrap(conn: Connection) -> Result<Self> {
        // WAL keeps the 1 Hz sampler from blocking dashboard reads.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        let mut db = Self { conn };
        db.migrate()?;
        db.seed_builtin_categories()?;
        Ok(db)
    }

    /// Escape hatch for queries not yet wrapped in a method.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

// ---------------------------------------------------------------------------
// Shared row codecs.
//
// These four functions are the ONLY place `SubjectRef`/`LimitTarget` values
// are translated to/from the `(subject_type|target_type, subject_id|target_id)`
// column pairs. Every table that stores a subject or a target uses the same
// encoding; keeping one codec each means a new variant cannot be added to
// half the call sites.
// ---------------------------------------------------------------------------

/// Encode a [`SubjectRef`] for the `subject_type` / `subject_id` columns.
fn subject_to_row(subject: SubjectRef) -> (&'static str, i64) {
    match subject {
        SubjectRef::App(id) => ("app", id),
        SubjectRef::Site(id) => ("site", id),
    }
}

/// Inverse of [`subject_to_row`]. Returns `None` for type strings no writer
/// of this schema emits; callers should skip such rows with a warning rather
/// than aborting the whole listing.
fn subject_from_row(kind: &str, id: i64) -> Option<SubjectRef> {
    match kind {
        "app" => Some(SubjectRef::App(id)),
        "site" => Some(SubjectRef::Site(id)),
        _ => None,
    }
}

/// Encode a [`LimitTarget`] for the `target_type` / `target_id` columns
/// (`target_id` is NULL for the total budget).
fn target_to_row(target: &LimitTarget) -> (&'static str, Option<i64>) {
    match target {
        LimitTarget::App(id) => ("app", Some(*id)),
        LimitTarget::Category(id) => ("category", Some(*id)),
        LimitTarget::Total => ("total", None),
    }
}

/// Inverse of [`target_to_row`]. Returns `None` for unknown encodings so
/// callers can skip-and-warn instead of failing the query outright.
fn row_to_target(kind: &str, id: Option<i64>) -> Option<LimitTarget> {
    match (kind, id) {
        ("app", Some(id)) => Some(LimitTarget::App(id)),
        ("category", Some(id)) => Some(LimitTarget::Category(id)),
        ("total", _) => Some(LimitTarget::Total),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    //! Fixtures shared by the per-module test suites.

    use chrono::{DateTime, Utc};
    use st_core::daykey::DayKey;
    use st_core::model::{SubjectRef, UsageInterval};

    pub(crate) fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-20T12:00:00Z")
            .expect("valid rfc3339")
            .with_timezone(&Utc)
    }

    pub(crate) fn interval(app_id: i64, secs: i64, day: DayKey) -> UsageInterval {
        UsageInterval {
            subject: SubjectRef::App(app_id),
            session_id: "s1".into(),
            start: now(),
            end: now() + chrono::Duration::seconds(secs),
            day_key: day,
        }
    }
}
