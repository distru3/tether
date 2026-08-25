//! Taxonomy: categories and the apps assigned to them.
//!
//! This is the part of storage the classifier and the UI editor touch. The
//! invariants that live here:
//!
//! * Built-in categories are re-seeded on every start so a new release can
//!   add one without a migration; user-created rows are never touched.
//! * A human classification (`user_classified`) must never be overwritten by
//!   an automatic re-classification — see [`Db::upsert_app`].
//!
//! The `CategoryKind <-> string` mapping at the bottom of this file is the
//! single codec for the `categories.kind` column. SQL that needs to compare
//! kinds binds `kind_to_str(..)` as a parameter instead of spelling the
//! literal again, so renaming a kind is a one-line change.

use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use st_core::category::{Category, CategoryKind, BUILTIN_CATEGORIES};
use st_core::model::{AppKey, AppRecord, CategoryId};

use crate::{Db, Result, StorageError};

/// The canonical wire/storage form of each [`CategoryKind`], matching the
/// CHECK constraint on `categories.kind`.
pub(crate) fn kind_to_str(kind: CategoryKind) -> &'static str {
    match kind {
        CategoryKind::Limitable => "limitable",
        CategoryKind::BlockOnly => "block_only",
        CategoryKind::NeverBlock => "never_block",
    }
}

/// Inverse of [`kind_to_str`]; `None` for strings outside the CHECK domain.
pub(crate) fn kind_from_str(s: &str) -> Option<CategoryKind> {
    match s {
        "limitable" => Some(CategoryKind::Limitable),
        "block_only" => Some(CategoryKind::BlockOnly),
        "never_block" => Some(CategoryKind::NeverBlock),
        _ => None,
    }
}

impl Db {
    /// Insert any built-in category that is missing.
    ///
    /// Idempotent, and runs on every start so that categories added in a later
    /// release appear without a migration.
    pub fn seed_builtin_categories(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO categories (slug, name, kind, color, builtin)
                 VALUES (?1, ?2, ?3, ?4, 1)
                 ON CONFLICT(slug) DO UPDATE SET
                     kind = excluded.kind,
                     color = excluded.color",
            )?;
            for c in BUILTIN_CATEGORIES {
                stmt.execute(params![c.slug, c.name, kind_to_str(c.kind), c.color])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn category_id(&self, slug: &str) -> Result<CategoryId> {
        self.conn
            .query_row(
                "SELECT id FROM categories WHERE slug = ?1",
                params![slug],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| StorageError::UnknownCategory(slug.to_string()))
    }

    /// The `kind` of a category, used to decide whether a limit may be attached.
    pub fn category_kind(&self, id: i64) -> Result<Option<CategoryKind>> {
        let kind_s: Option<String> = self
            .conn
            .query_row(
                "SELECT kind FROM categories WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(kind_s.and_then(|s| kind_from_str(&s)))
    }

    /// Record that an app was seen, returning its id.
    ///
    /// Never touches `primary_category_id` on an existing row: re-classifying
    /// silently would throw away the user's decision and their limits with it.
    pub fn upsert_app(
        &self,
        key: &AppKey,
        display_name: &str,
        publisher: Option<&str>,
        default_category: CategoryId,
        now: DateTime<Utc>,
    ) -> Result<i64> {
        let now_s = now.to_rfc3339();
        let id = self.conn.query_row(
            "INSERT INTO apps
                 (app_key, display_name, publisher, primary_category_id,
                  first_seen_utc, last_seen_utc)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(app_key) DO UPDATE SET
                 last_seen_utc = excluded.last_seen_utc,
                 display_name  = excluded.display_name
             RETURNING id",
            params![
                key.to_db_string(),
                display_name,
                publisher,
                default_category,
                now_s
            ],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    /// Current classification of an app: its primary category and whether a
    /// human chose it. Lets the classifier skip rows it already owns, so it
    /// never rewrites the same verdict on every interval.
    pub fn app_category_state(&self, app_id: i64) -> Result<(CategoryId, bool)> {
        self.conn
            .query_row(
                "SELECT primary_category_id, user_classified FROM apps WHERE id = ?1",
                params![app_id],
                |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0)),
            )
            .optional()?
            .ok_or_else(|| StorageError::AppNotFound(app_id))
    }

    pub fn set_app_categories(
        &mut self,
        app_id: i64,
        primary: CategoryId,
        tags: &[CategoryId],
        by_user: bool,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE apps SET primary_category_id = ?2, user_classified = ?3 WHERE id = ?1",
            params![app_id, primary, by_user as i64],
        )?;
        tx.execute("DELETE FROM app_tags WHERE app_id = ?1", params![app_id])?;
        {
            let mut stmt =
                tx.prepare("INSERT OR IGNORE INTO app_tags (app_id, category_id) VALUES (?1, ?2)")?;
            for tag in tags {
                if *tag != primary {
                    stmt.execute(params![app_id, tag])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// All apps known to the system, with their current classification.
    pub fn list_apps(&self) -> Result<Vec<AppRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.id, a.app_key, a.display_name, a.publisher,
                    a.primary_category_id, a.user_classified
             FROM apps a
             ORDER BY a.display_name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)? != 0,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, key_s, display_name, publisher, primary_category, user_classified) = row?;
            let Some(key) = AppKey::parse_db_string(&key_s) else {
                tracing::warn!(app_key = %key_s, "skipping app with unparsable key");
                continue;
            };
            let tags = self.app_tags(id)?;
            out.push(AppRecord {
                id,
                key,
                display_name,
                publisher,
                primary_category,
                tags,
                user_classified,
            });
        }
        Ok(out)
    }

    /// The database id of an app given its canonical key, if it has been seen.
    pub fn app_id_for_key(&self, key: &AppKey) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id FROM apps WHERE app_key = ?1",
                params![key.to_db_string()],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// A single app record, for the enforcement loop and the categoriser.
    ///
    /// Error semantics (deliberate): `Ok(None)` means *this row does not
    /// exist* (or its stored key is unparsable). Any query failure — locked
    /// database, corrupt page, missing table — is returned as `Err`. Callers
    /// treat `None` as "nothing to enforce", so folding real errors into None
    /// would silently switch enforcement off.
    pub fn app_record(&self, app_id: i64) -> Result<Option<AppRecord>> {
        let row = self.conn.query_row(
            "SELECT a.id, a.app_key, a.display_name, a.publisher,
                    a.primary_category_id, a.user_classified
             FROM apps a WHERE a.id = ?1",
            params![app_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)? != 0,
                ))
            },
        );
        let (id, key_s, display_name, publisher, primary_category, user_classified) = match row {
            Ok(values) => values,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(source) => return Err(source.into()),
        };
        let Some(key) = AppKey::parse_db_string(&key_s) else {
            tracing::warn!(app_id, app_key = %key_s, "app row has an unparsable key");
            return Ok(None);
        };
        Ok(Some(AppRecord {
            id,
            key,
            display_name,
            publisher,
            primary_category,
            tags: self.app_tags(id)?,
            user_classified,
        }))
    }

    fn app_tags(&self, app_id: i64) -> Result<Vec<CategoryId>> {
        let mut stmt = self
            .conn
            .prepare("SELECT category_id FROM app_tags WHERE app_id = ?1 ORDER BY category_id")?;
        let rows = stmt.query_map(params![app_id], |row| row.get(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// All categories, so the UI can render the editor without hard-coding ids.
    pub fn list_categories(&self) -> Result<Vec<Category>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, slug, name, kind, color, builtin FROM categories ORDER BY id")?;
        let rows = stmt.query_map([], |row| {
            let kind_s: String = row.get(3)?;
            Ok(Category {
                id: row.get(0)?,
                slug: row.get(1)?,
                name: row.get(2)?,
                kind: kind_from_str(&kind_s)
                    .unwrap_or(CategoryKind::Limitable)
                    .to_owned(),
                color: row.get(4)?,
                builtin: row.get::<_, i64>(5)? != 0,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::params;
    use st_core::category::CategoryKind;
    use st_core::limits::LimitTarget;
    use st_core::model::AppKey;

    use super::*;
    use crate::testutil::now;

    #[test]
    fn upsert_app_preserves_a_user_category() {
        let mut db = Db::open_in_memory().expect("open");
        let games = db.category_id("games").expect("games");
        let uncat = db.category_id("uncategorized").expect("uncategorized");

        let key = AppKey::windows_exe("C:\\Games\\Steam\\steam.exe");
        let id = db
            .upsert_app(&key, "Steam", None, uncat, now())
            .expect("insert");
        db.set_app_categories(id, games, &[], true)
            .expect("classify");

        // Seeing the app again must not reset the category.
        let same = db
            .upsert_app(&key, "Steam", None, uncat, now())
            .expect("upsert");
        assert_eq!(id, same);

        let (cat, user): (i64, i64) = db
            .conn()
            .query_row(
                "SELECT primary_category_id, user_classified FROM apps WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("read back");
        assert_eq!(cat, games);
        assert_eq!(user, 1);
    }

    #[test]
    fn app_key_normalisation_prevents_duplicate_rows() {
        let db = Db::open_in_memory().expect("open");
        let uncat = db.category_id("uncategorized").expect("uncategorized");

        let a = db
            .upsert_app(
                &AppKey::windows_exe("C:/Apps/Foo.exe"),
                "Foo",
                None,
                uncat,
                now(),
            )
            .expect("a");
        let b = db
            .upsert_app(
                &AppKey::windows_exe("c:\\apps\\foo.exe"),
                "Foo",
                None,
                uncat,
                now(),
            )
            .expect("b");
        assert_eq!(a, b, "case and separator differences must collapse");
    }

    #[test]
    fn app_record_returns_tags_and_classification() {
        let mut db = Db::open_in_memory().expect("open");
        let social = db.category_id("social-media").expect("social");
        let shortform = db.category_id("short-form-video").expect("shortform");
        let uncat = db.category_id("uncategorized").expect("uncat");

        let id = db
            .upsert_app(
                &AppKey::windows_exe("C:\\tiktok.exe"),
                "TikTok",
                None,
                uncat,
                now(),
            )
            .expect("app");
        db.set_app_categories(id, social, &[shortform], false)
            .expect("classify");

        let record = db.app_record(id).expect("record").expect("some");
        assert_eq!(record.primary_category, social);
        assert_eq!(record.tags, vec![shortform]);
        assert!(!record.user_classified);

        assert!(db.app_record(9999).expect("none").is_none());
    }

    /// Regression guard for the audit finding: only *no such row* may come
    /// back as `None`. A broken table must surface as an error, because the
    /// enforcer treats `None` as "nothing to do".
    #[test]
    fn app_record_surfaces_database_errors_instead_of_looking_missing() {
        let db = Db::open_in_memory().expect("open");
        // Fresh install seeds categories but no apps, so dropping the table
        // leaves nothing to violate foreign keys.
        db.conn()
            .execute_batch("DROP TABLE apps")
            .expect("break the schema through the public escape hatch");

        let err = db
            .app_record(42)
            .expect_err("query failure must propagate, not become Ok(None)");
        assert!(
            matches!(err, crate::StorageError::Sqlite(_)),
            "expected a sqlite error, got {err:?}"
        );
    }

    #[test]
    fn app_record_returns_none_only_for_a_genuinely_missing_row() {
        let db = Db::open_in_memory().expect("open");
        let uncat = db.category_id("uncategorized").expect("uncat");
        let id = db
            .upsert_app(
                &AppKey::windows_exe("C:\\real.exe"),
                "Real",
                None,
                uncat,
                now(),
            )
            .expect("app");

        // An existing id yields Some; an unused id yields None without error.
        assert!(db.app_record(id).expect("existing").is_some());
        assert!(db
            .app_record(id + 10_000)
            .expect("missing row is not an error")
            .is_none());
    }

    #[test]
    fn list_categories_exposes_kinds_for_the_editor() {
        let db = Db::open_in_memory().expect("open");
        let cats = db.list_categories().expect("list");
        let dev = cats.iter().find(|c| c.slug == "development").expect("dev");
        assert_eq!(dev.kind, CategoryKind::NeverBlock);
        let games = cats.iter().find(|c| c.slug == "games").expect("games");
        assert_eq!(games.kind, CategoryKind::Limitable);
    }

    /// The shared row codecs round-trip every variant and reject unknown
    /// encodings, so skip-and-warn call sites have a reliable `None`.
    #[test]
    fn row_codecs_round_trip_subjects_and_targets() {
        use st_core::model::SubjectRef;

        use crate::{row_to_target, subject_from_row, subject_to_row, target_to_row};

        for id in [1_i64, 99] {
            for (subject, kind) in [(SubjectRef::App(id), "app"), (SubjectRef::Site(id), "site")] {
                let (st, sid) = subject_to_row(subject);
                assert_eq!((st, sid), (kind, id));
                assert_eq!(subject_from_row(st, sid), Some(subject));
            }
        }
        for target in [
            LimitTarget::App(3),
            LimitTarget::Category(4),
            LimitTarget::Total,
        ] {
            let (tt, tid) = target_to_row(&target);
            assert_eq!(row_to_target(tt, tid), Some(target));
        }
        assert_eq!(row_to_target("widget", Some(1)), None);
        assert_eq!(subject_from_row("gadget", 1), None);
    }
}
