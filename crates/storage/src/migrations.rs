//! Ordered schema migrations.
//!
//! WHY a hand-rolled list instead of a migration crate: the schema is small,
//! and `PRAGMA user_version` is atomic with the migration's transaction, so
//! the whole mechanism fits in twenty lines we can audit. Append only; never
//! edit a shipped migration, or an upgraded install will silently diverge
//! from a fresh one.

use crate::{Db, Result};

/// Ordered migrations keyed by the `user_version` they produce.
pub(crate) const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../migrations/0001_init.sql")),
    (2, include_str!("../migrations/0002_pending_limits.sql")),
];

impl Db {
    /// Apply every migration newer than the database's `user_version`.
    ///
    /// Each migration runs in its own transaction together with the version
    /// bump, so a crash halfway leaves either the old schema or the new one,
    /// never a mixture.
    pub(crate) fn migrate(&mut self) -> Result<()> {
        let current: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;

        for (version, sql) in MIGRATIONS {
            if *version <= current {
                continue;
            }
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", *version)?;
            tx.commit()?;
            tracing::info!(version, "applied migration");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rusqlite::Connection;

    use super::MIGRATIONS;
    use crate::Db;

    /// A unique temp directory per test; no `tempfile` dependency wanted.
    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("st-storage-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn migrations_and_seed_are_idempotent() {
        let mut db = Db::open_in_memory().expect("open");
        db.seed_builtin_categories().expect("reseed");
        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count as usize, st_core::category::BUILTIN_CATEGORIES.len());
    }

    /// An install upgraded on disk must end up at the latest version without
    /// losing pre-existing rows. This exercises the real `Db::open` path —
    /// every other test uses an already-migrated in-memory database, which
    /// would never notice a broken upgrade.
    #[test]
    fn file_db_upgrade_applies_pending_migrations_and_preserves_data() {
        let dir = temp_dir("upgrade");
        let path = dir.join("upgrade.db");

        // Build a v1-schema database by applying only the first migration,
        // then write a row that has to survive the upgrade.
        {
            let conn = Connection::open(&path).expect("create legacy db");
            conn.execute_batch(MIGRATIONS[0].1).expect("apply v1 only");
            conn.pragma_update(None, "user_version", 1).expect("v1");
            conn.execute(
                "INSERT INTO categories (slug, name, kind, color, builtin)
                 VALUES ('legacy-slug', 'Legacy', 'limitable', '#123456', 0)",
                [],
            )
            .expect("seed legacy row");
        }

        let db = Db::open(&path).expect("reopen and upgrade");

        let version: i64 = db
            .conn()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("version");
        assert_eq!(version, 2, "upgrade must reach the latest migration");

        // The 0002 table now exists and is empty but usable.
        let pending: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM pending_limits", [], |r| r.get(0))
            .expect("pending_limits must exist after upgrade");
        assert_eq!(pending, 0);

        // Pre-upgrade data survived, and the bootstrap seed still ran.
        let legacy = db.category_id("legacy-slug").expect("legacy row survives");
        assert!(legacy > 0);
        assert!(db.category_id("uncategorized").is_ok(), "builtins seeded");

        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
