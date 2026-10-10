//! Web filtering storage: manual domain blocks and the site catalog.
//!
//! What is live: `block_rules` rows with neither a blocklist nor a category
//! are the user's manual blocks. The agent's main loop pushes them to the
//! hosts file (`st-enforce-win::HostsFileFilter`); IPC adds and removes them.
//! `sites` / `site_tags` back the allowlist's site names.
//!
//! The schema also has `blocklists` (and the `blocklist_id` / `category_id`
//! columns on `block_rules`) for imported lists. Nothing writes or enforces
//! them today; the functions that once managed them were removed as dead code
//! in 2026-10. The tables stay because shipped migrations are immutable.

use rusqlite::{params, OptionalExtension};
use st_core::model::{CategoryId, SiteRecord};

use crate::{Db, Result, StorageError};

/// One row of `block_rules`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRuleRow {
    pub id: i64,
    pub blocklist_id: Option<i64>,
    pub category_id: Option<i64>,
    pub domain: String,
    pub include_subdomains: bool,
    pub action: String,
}

impl Db {
    /// Add one rule. `action` is `'block' | 'allow'`; anything else is refused
    /// here with a clear message instead of letting the CHECK constraint fire.
    /// Returns the new rule id.
    pub fn add_block_rule(
        &self,
        blocklist_id: Option<i64>,
        category_id: Option<i64>,
        domain: &str,
        include_subdomains: bool,
        action: &str,
    ) -> Result<i64> {
        if action != "block" && action != "allow" {
            return Err(StorageError::Invalid(format!(
                "unknown rule action '{action}' (expected 'block' or 'allow')"
            )));
        }
        let Some(domain) = normalize_domain(domain) else {
            return Err(StorageError::Invalid("domain is empty or malformed".into()));
        };
        let id = self.conn.query_row(
            "INSERT INTO block_rules (blocklist_id, category_id, domain, include_subdomains, action)
             VALUES (?1, ?2, ?3, ?4, ?5)
             RETURNING id",
            params![blocklist_id, category_id, domain, include_subdomains as i64, action],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    pub fn list_block_rules(&self) -> Result<Vec<BlockRuleRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, blocklist_id, category_id, domain, include_subdomains, action
             FROM block_rules ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(BlockRuleRow {
                id: row.get(0)?,
                blocklist_id: row.get(1)?,
                category_id: row.get(2)?,
                domain: row.get(3)?,
                include_subdomains: row.get::<_, i64>(4)? != 0,
                action: row.get(5)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn delete_block_rule(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM block_rules WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Upsert a site keyed by (normalised) domain. Re-categorising via this call
    /// is a human choice (`user_classified` stays true); the caller passes the
    /// primary category the user picked.
    pub fn upsert_site(&self, domain: &str, primary_category: CategoryId) -> Result<i64> {
        let Some(domain) = normalize_domain(domain) else {
            return Err(StorageError::Invalid("domain is empty or malformed".into()));
        };
        let id = self.conn.query_row(
            "INSERT INTO sites (domain, primary_category_id, user_classified)
             VALUES (?1, ?2, 1)
             ON CONFLICT(domain) DO UPDATE SET primary_category_id = excluded.primary_category_id
             RETURNING id",
            params![domain, primary_category],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    /// Current classification of a site: primary category and whether a human
    /// chose it.
    pub fn site_category(&self, site_id: i64) -> Result<(CategoryId, bool)> {
        self.conn
            .query_row(
                "SELECT primary_category_id, user_classified FROM sites WHERE id = ?1",
                params![site_id],
                |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0)),
            )
            .optional()?
            .ok_or_else(|| StorageError::Invalid(format!("unknown site id {site_id}")))
    }

    pub fn set_site_tags(
        &mut self,
        site_id: i64,
        primary: CategoryId,
        tags: &[CategoryId],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE sites SET primary_category_id = ?2, user_classified = 1 WHERE id = ?1",
            params![site_id, primary],
        )?;
        tx.execute("DELETE FROM site_tags WHERE site_id = ?1", params![site_id])?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO site_tags (site_id, category_id) VALUES (?1, ?2)",
            )?;
            for tag in tags {
                if *tag != primary {
                    stmt.execute(params![site_id, tag])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// All known sites with their classification, for the site/category editor.
    pub fn list_sites(&self) -> Result<Vec<SiteRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, domain, primary_category_id, user_classified FROM sites ORDER BY domain",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)? != 0,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, domain, primary_category, user_classified) = row?;
            out.push(SiteRecord {
                id,
                domain,
                primary_category,
                tags: self.site_tags(id)?,
                user_classified,
            });
        }
        Ok(out)
    }

    fn site_tags(&self, site_id: i64) -> Result<Vec<CategoryId>> {
        let mut stmt = self
            .conn
            .prepare("SELECT category_id FROM site_tags WHERE site_id = ?1 ORDER BY category_id")?;
        let rows = stmt.query_map(params![site_id], |row| row.get(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}

/// Normalise a domain for storage: lower-case, strip a leading and a trailing
/// dot, reject blank and internally-empty labels. Returns `None` when nothing
/// usable remains. This is the same shape the resolver's `resolve::normalize`
/// applies, so the two never disagree about how a name is written.
pub fn normalize_domain(domain: &str) -> Option<String> {
    let mut s = domain.trim().to_ascii_lowercase();
    if s.starts_with('.') {
        s.remove(0);
    }
    while s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() || s.contains("..") {
        return None;
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return None;
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_block_rule_normalises_and_validates() {
        let db = Db::open_in_memory().expect("open");
        let id = db
            .add_block_rule(None, None, "  .Example.com.  ", true, "block")
            .expect("add");
        let row = db
            .list_block_rules()
            .expect("rules")
            .into_iter()
            .next()
            .expect("one");
        assert_eq!(row.id, id);
        assert_eq!(row.domain, "example.com");
        assert!(row.include_subdomains);

        assert!(db
            .add_block_rule(None, None, "bad domain", true, "block")
            .is_err());
        assert!(db
            .add_block_rule(None, None, "example.com", true, "nonsense")
            .is_err());
    }

    #[test]
    fn site_crud_and_tag_management_round_trip() {
        let mut db = Db::open_in_memory().expect("open");
        let social = db.category_id("social-media").expect("social");
        let shortform = db.category_id("short-form-video").expect("shortform");

        let site = db.upsert_site("tiktok.com", social).expect("upsert");
        db.set_site_tags(site, social, &[shortform]).expect("tags");
        // Re-upserting the same domain returns the same id (UNIQUE domain).
        let again = db.upsert_site("TikTok.com", social).expect("re-upsert");
        assert_eq!(site, again);

        let (cat, user) = db.site_category(site).expect("cat");
        assert_eq!(cat, social);
        assert!(user, "upsert_site marks it user-classified");

        let sites = db.list_sites().expect("sites");
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].domain, "tiktok.com");
        assert_eq!(sites[0].tags, vec![shortform]);
    }

    #[test]
    fn normalize_domain_rejects_malformed_input() {
        assert_eq!(
            normalize_domain("  .Example.COM.  ").as_deref(),
            Some("example.com")
        );
        assert_eq!(normalize_domain(""), None);
        assert_eq!(normalize_domain(".."), None);
        assert_eq!(normalize_domain("a..b.com"), None);
        assert_eq!(normalize_domain("bad domain"), None);
    }
}
