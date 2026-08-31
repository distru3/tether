//! Web filtering: the dead-but-present `blocklists` / `block_rules` /
//! `sites` / `site_tags` tables, wired into real methods.
//!
//! The 0001 schema shipped these tables ahead of the feature ([`migrations`]
//! deliberately left them untouched — they are NOT migrated here; the columns
//! were already correct). This module is the storage half of the Phase-1 DNS
//! proxy: manage blocklists, edit their rules and the site/category editor, and
//! compute the one set the resolver actually enforces.
//!
//! # The allow-wins resolution (read this before touching it)
//!
//! [`Db::collect_active_block_rules`] is the only place the two contradictory
//! rule actions meet. `block_rules.action` is `'block' | 'allow'`; an `allow`
//! must win over a `block` without leaking contradictory pairs to the resolver
//! (which only ever understands "block these"). The algorithm:
//!
//! 1. Load every rule from an **enabled** blocklist (a rule with no blocklist is
//!    always live).
//! 2. Split into block rules `B` and allowance rules `A`.
//! 3. A domain is *allowed* if any `A` covers it — `A` covers `X` when `X == A.domain`
//!    or (`A.include_subdomains` and `X` is a strict subdomain of `A.domain`).
//! 4. Emit each `B` unless:
//!    * the exact host `B.domain` is allowed → the whole block is dropped
//!      (allow-wins over the root), or
//!    * `B.include_subdomains` is true and some *strict subdomain* of `B.domain`
//!      is allowed. `BlockRule` cannot express a single-host hole in a wildcard
//!      (that would need an exclusion set), so **Phase 1 degrades: the wildcard
//!      is downgraded to an exact-host block.** The allowed host and its siblings
//!      are then no longer covered by the wildcard.
//!
//! Step 4's downgrade is a documented Phase-1 limitation, not an oversight: the
//! resolver's rule type is `(domain, include_subdomains)` and has no hole, and
//! `st-core` cannot be changed. The alternative — keeping the wildcard and
//! leaving the allowed host blocked — would silently ignore the user's explicit
//! override, which is worse.

use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use st_core::model::{CategoryId, SiteRecord};
use st_core::platform::BlockRule;

use crate::{Db, Result, StorageError};

/// Curated seed domains for the default adult content filter.
pub const DEFAULT_ADULT_DOMAINS: &[&str] = &[
    "pornhub.com",
    "xvideos.com",
    "xnxx.com",
    "xhamster.com",
    "onlyfans.com",
    "redtube.com",
    "youporn.com",
    "chaturbate.com",
    "stripchat.com",
    "livejasmin.com",
    "cam4.com",
    "bongacams.com",
    "spankbang.com",
    "tube8.com",
    "beeg.com",
    "brazzers.com",
    "eporner.com",
    "txxx.com",
    "hqporner.com",
    "tnaflix.com",
    "heavy-r.com",
    "motherless.com",
    "rule34.xxx",
    "e-hentai.org",
    "nhentai.net",
    "danbooru.donmai.us",
    "gelbooru.com",
    "fapdu.com",
    "extremetube.com",
    "sunporno.com",
    "keezmovies.com",
    "fakku.net",
    "hanime.tv",
    "manyvids.com",
    "fansly.com",
    "camsoda.com",
    "adultfriendfinder.com",
    "flirt4free.com",
    "myfreecams.com",
    "imlive.com",
    "xrated.com",
    "playboy.com",
    "penthouse.com",
    "hustler.com",
    "bangbros.com",
    "naughtyamerica.com",
    "realitykings.com",
    "mofos.com",
    "twistys.com",
    "wicked.com",
    "digitalplayground.com",
    "rk.com",
    "clips4sale.com",
    "porn.com",
    "hardsextube.com",
    "empflix.com",
    "4tube.com",
    "fuq.com",
    "nuvid.com",
];

/// Curated seed domains for the default social media filter.
pub const DEFAULT_SOCIAL_MEDIA_DOMAINS: &[&str] = &[
    "facebook.com",
    "fb.com",
    "instagram.com",
    "tiktok.com",
    "twitter.com",
    "x.com",
    "reddit.com",
    "snapchat.com",
    "pinterest.com",
    "linkedin.com",
    "discord.com",
    "threads.net",
    "tumblr.com",
    "twitch.tv",
    "bsky.app",
    "blueskyweb.xyz",
    "mastodon.social",
    "weibo.com",
    "vk.com",
    "t.co",
    "fbcdn.net",
    "cdninstagram.com",
    "tiktokcdn.com",
    "twimg.com",
    "redd.it",
    "redditmedia.com",
];

/// One row of `blocklists`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlocklistRow {
    pub id: i64,
    pub name: String,
    pub source_url: Option<String>,
    pub version: Option<String>,
    pub checksum: Option<String>,
    pub enabled: bool,
    pub last_updated_utc: Option<String>,
}

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
    /// Create a blocklist, returning its id. `last_updated_utc` is left NULL
    /// until the list is actually refreshed (import sets it).
    pub fn create_blocklist(
        &self,
        name: &str,
        source_url: Option<&str>,
        checksum: Option<&str>,
    ) -> Result<i64> {
        let id = self.conn.query_row(
            "INSERT INTO blocklists (name, source_url, checksum)
             VALUES (?1, ?2, ?3)
             RETURNING id",
            params![name, source_url, checksum],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    pub fn list_blocklists(&self) -> Result<Vec<BlocklistRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, source_url, version, checksum, enabled, last_updated_utc
             FROM blocklists ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(BlocklistRow {
                id: row.get(0)?,
                name: row.get(1)?,
                source_url: row.get(2)?,
                version: row.get(3)?,
                checksum: row.get(4)?,
                enabled: row.get::<_, i64>(5)? != 0,
                last_updated_utc: row.get(6)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// One blocklist, or `None` when it does not exist.
    pub fn blocklist(&self, id: i64) -> Result<Option<BlocklistRow>> {
        self.conn
            .query_row(
                "SELECT id, name, source_url, version, checksum, enabled, last_updated_utc
                 FROM blocklists WHERE id = ?1",
                params![id],
                |row| {
                    Ok(BlocklistRow {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        source_url: row.get(2)?,
                        version: row.get(3)?,
                        checksum: row.get(4)?,
                        enabled: row.get::<_, i64>(5)? != 0,
                        last_updated_utc: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn set_blocklist_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE blocklists SET enabled = ?2 WHERE id = ?1",
            params![id, enabled as i64],
        )?;
        Ok(())
    }

    /// Delete a blocklist; its rules are removed by the FK cascade.
    pub fn delete_blocklist(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM blocklists WHERE id = ?1", params![id])?;
        Ok(())
    }

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

    /// Bulk-add fetched domains to a blocklist, normalising each and skipping
    /// blanks/invalid labels. Updates the list's `last_updated_utc` and returns
    /// how many rules were inserted.
    pub fn import_blocklist_domains(
        &mut self,
        blocklist_id: i64,
        domains: &[String],
        action: &str,
        now: DateTime<Utc>,
    ) -> Result<usize> {
        if action != "block" && action != "allow" {
            return Err(StorageError::Invalid(format!(
                "unknown rule action '{action}' (expected 'block' or 'allow')"
            )));
        }
        let tx = self.conn.transaction()?;
        let mut inserted = 0usize;
        {
            // `block_rules` has no UNIQUE on (blocklist_id, domain), so
            // re-importing a refreshed list would otherwise double every rule.
            // Guard with NOT EXISTS (scoped per blocklist) to make imports
            // idempotent; the same domain in two different lists is still two
            // rows, as a user may intentionally maintain overlapping lists.
            let mut stmt = tx.prepare(
                "INSERT INTO block_rules (blocklist_id, category_id, domain, include_subdomains, action)
                 SELECT ?1, NULL, ?2, 1, ?3
                 WHERE NOT EXISTS (
                     SELECT 1 FROM block_rules WHERE blocklist_id = ?1 AND domain = ?2
                 )",
            )?;
            const MAX_IMPORT_DOMAINS: usize = 300;
            let safe_domains = if domains.len() > MAX_IMPORT_DOMAINS {
                &domains[..MAX_IMPORT_DOMAINS]
            } else {
                domains
            };

            for domain in safe_domains {
                let Some(domain) = normalize_domain(domain) else {
                    continue;
                };
                let n = stmt.execute(params![blocklist_id, domain, action])?;
                inserted += n;
            }
        }
        tx.execute(
            "UPDATE blocklists SET last_updated_utc = ?2 WHERE id = ?1",
            params![blocklist_id, now.to_rfc3339()],
        )?;
        tx.commit()?;
        Ok(inserted)
    }

    /// Seed the default curated adult content and social media blocklists.
    ///
    /// Idempotent: creates the "Adult Content Filter" and "Social Media Filter"
    /// blocklists if not already present and populates them with curated domains.
    /// Prunes any oversized legacy entries so the database and rule set remain lightweight.
    pub fn seed_default_blocklists(&mut self) -> Result<()> {
        let adult_cat_id = self.category_id("adult-content").ok();
        let social_cat_id = self.category_id("social-media").ok();
        let tx = self.conn.transaction()?;
        {
            // 1. Adult Content Filter
            tx.execute(
                "INSERT INTO blocklists (name, source_url, enabled)
                 VALUES (?1, ?2, 1)
                 ON CONFLICT(name) DO NOTHING",
                params![
                    "Adult Content Filter",
                    "https://raw.githubusercontent.com/StevenBlack/hosts/master/alternates/porn/hosts"
                ],
            )?;

            let adult_bl_id: i64 = tx.query_row(
                "SELECT id FROM blocklists WHERE name = ?1",
                params!["Adult Content Filter"],
                |r| r.get(0),
            )?;

            // Clean any legacy bloated rules (e.g. 80,000+ entries) from previous imports
            tx.execute(
                "DELETE FROM block_rules WHERE blocklist_id = ?1",
                params![adult_bl_id],
            )?;

            let mut adult_stmt = tx.prepare(
                "INSERT INTO block_rules (blocklist_id, category_id, domain, include_subdomains, action)
                 VALUES (?1, ?2, ?3, 1, 'block')",
            )?;

            for domain in DEFAULT_ADULT_DOMAINS {
                let Some(norm) = normalize_domain(domain) else {
                    continue;
                };
                adult_stmt.execute(params![adult_bl_id, adult_cat_id, norm])?;
            }

            // 2. Social Media Filter
            tx.execute(
                "INSERT INTO blocklists (name, source_url, enabled)
                 VALUES (?1, ?2, 1)
                 ON CONFLICT(name) DO NOTHING",
                params![
                    "Social Media Filter",
                    "https://raw.githubusercontent.com/StevenBlack/hosts/master/alternates/social/hosts"
                ],
            )?;

            let social_bl_id: i64 = tx.query_row(
                "SELECT id FROM blocklists WHERE name = ?1",
                params!["Social Media Filter"],
                |r| r.get(0),
            )?;

            // Clean any legacy bloated rules from previous imports
            tx.execute(
                "DELETE FROM block_rules WHERE blocklist_id = ?1",
                params![social_bl_id],
            )?;

            let mut social_stmt = tx.prepare(
                "INSERT INTO block_rules (blocklist_id, category_id, domain, include_subdomains, action)
                 VALUES (?1, ?2, ?3, 1, 'block')",
            )?;

            for domain in DEFAULT_SOCIAL_MEDIA_DOMAINS {
                let Some(norm) = normalize_domain(domain) else {
                    continue;
                };
                social_stmt.execute(params![social_bl_id, social_cat_id, norm])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The effective set the DNS resolver should enforce: every block rule from
    /// an enabled blocklist or an active category limit / focus block, allow-resolved
    /// so contradictory `allow` rows never reach the resolver. See the module docs for the algorithm.
    pub fn collect_active_block_rules(&self) -> Result<Vec<BlockRule>> {
        let mut stmt = self.conn.prepare(
            "SELECT br.domain, br.include_subdomains, br.action
             FROM block_rules br
             LEFT JOIN blocklists b ON b.id = br.blocklist_id
             WHERE (
                 b.enabled = 1
                 OR br.blocklist_id IS NULL
                 OR (
                     br.category_id IS NOT NULL
                     AND EXISTS (
                         SELECT 1 FROM block_state bs
                         WHERE bs.subject_type = 'category' AND bs.subject_id = br.category_id
                     )
                 )
             )",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? != 0,
                row.get::<_, String>(2)?,
            ))
        })?;

        let mut blocks: Vec<(String, bool)> = Vec::new();
        let mut allows: Vec<(String, bool)> = Vec::new();
        for row in rows {
            let (raw_domain, include, action) = row?;
            let Some(domain) = normalize_domain(&raw_domain) else {
                continue;
            };
            if action == "allow" {
                allows.push((domain, include));
            } else {
                blocks.push((domain, include));
            }
        }

        let mut out = Vec::with_capacity(blocks.len());
        for (domain, include) in blocks {
            // Exact host allowed? Drop the entire block (allow-wins over the root).
            if allows
                .iter()
                .any(|(root, inc)| matches(&domain, root, *inc))
            {
                continue;
            }
            // Wildcard with an allow inside its subtree: `BlockRule` has no hole,
            // so Phase 1 downgrades the wildcard to an exact-host block.
            if include
                && allows
                    .iter()
                    .any(|(root, _)| is_strict_subdomain(root, &domain))
            {
                out.push(BlockRule {
                    domain,
                    include_subdomains: false,
                });
                continue;
            }
            out.push(BlockRule {
                domain,
                include_subdomains: include,
            });
        }
        Ok(out)
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

/// Does `name` fall under `root`, with subdomains covered only when `include`?
/// Mirrors `st-dns::resolve`; kept local so `st-storage` never needs to depend
/// on the DNS crate to compute the same answer.
fn matches(name: &str, root: &str, include: bool) -> bool {
    if name == root {
        return true;
    }
    include
        && name.len() > root.len()
        && name.ends_with(root)
        && name.as_bytes()[name.len() - root.len() - 1] == b'.'
}

/// Is `sub` a strict (multi-label) subdomain of `root`? e.g. `a.example.com`
/// under `example.com`, but NOT `example.com` under itself nor `badexample.com`.
fn is_strict_subdomain(sub: &str, root: &str) -> bool {
    sub != root
        && sub.len() > root.len()
        && sub.ends_with(root)
        && sub.as_bytes()[sub.len() - root.len() - 1] == b'.'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::now;

    fn block(domain: &str, sub: bool) -> BlockRule {
        BlockRule {
            domain: domain.into(),
            include_subdomains: sub,
        }
    }

    #[test]
    fn blocklist_crud_round_trips_and_cascades_rules() {
        let db = Db::open_in_memory().expect("open");
        let id = db
            .create_blocklist(
                "porn",
                Some("https://example.invalid/list.txt"),
                Some("abc"),
            )
            .expect("create");
        assert!(db.create_blocklist("dupe", None, None).is_ok());
        // Name is UNIQUE in the schema.
        assert!(db.create_blocklist("porn", None, None).is_err());

        db.add_block_rule(Some(id), None, "porn.example", true, "block")
            .expect("rule");
        assert_eq!(db.list_block_rules().expect("rules").len(), 1);

        db.set_blocklist_enabled(id, false).expect("disable");
        let lists = db.list_blocklists().expect("list");
        assert_eq!(lists.len(), 2);
        let disabled = lists.iter().find(|b| b.id == id).expect("find");
        assert!(!disabled.enabled);
        // collect_active must now omit the disabled list's rule.
        assert_eq!(db.collect_active_block_rules().expect("active").len(), 0);
        db.set_blocklist_enabled(id, true).expect("enable");
        assert_eq!(db.collect_active_block_rules().expect("active").len(), 1);

        db.delete_blocklist(id).expect("delete");
        assert!(
            db.list_block_rules().expect("rules after").is_empty(),
            "rule cascade-deleted"
        );
        assert!(db.collect_active_block_rules().expect("active").is_empty());
    }

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
    fn a_stateful_rule_with_no_blocklist_is_always_active() {
        let db = Db::open_in_memory().expect("open");
        db.add_block_rule(None, None, "evil.example", true, "block")
            .expect("add");
        assert_eq!(
            db.collect_active_block_rules().expect("active"),
            vec![block("evil.example", true)]
        );
    }

    #[test]
    fn allow_resolves_an_exact_host_out_of_a_wildcard_block() {
        let db = Db::open_in_memory().expect("open");
        let bl = db.create_blocklist("social", None, None).expect("bl");
        db.add_block_rule(Some(bl), None, "example.com", true, "block")
            .expect("block");
        db.add_block_rule(Some(bl), None, "a.example.com", false, "allow")
            .expect("allow");

        let effective = db.collect_active_block_rules().expect("active");
        let single = effective.as_slice();
        assert_eq!(single, &[block("example.com", false)]);
    }

    #[test]
    fn allow_on_the_root_drops_the_whole_wildcard_block() {
        let db = Db::open_in_memory().expect("open");
        let bl = db.create_blocklist("adult", None, None).expect("bl");
        db.add_block_rule(Some(bl), None, "example.com", true, "block")
            .expect("block");
        // Allowing the exact root (with its own subdomain share) unblocks it all.
        db.add_block_rule(Some(bl), None, "example.com", true, "allow")
            .expect("allow");
        assert!(db.collect_active_block_rules().expect("active").is_empty());
    }

    #[test]
    fn a_parent_allow_wins_over_an_exact_block() {
        let db = Db::open_in_memory().expect("open");
        let bl = db.create_blocklist("b", None, None).expect("bl");
        db.add_block_rule(Some(bl), None, "a.example.com", false, "block")
            .expect("block");
        db.add_block_rule(Some(bl), None, "example.com", true, "allow")
            .expect("allow");
        // The parent allow covers a.example.com, so the block is dropped.
        assert!(db.collect_active_block_rules().expect("active").is_empty());
    }

    #[test]
    fn unrelated_rules_survive_an_elsewhere_allow() {
        let db = Db::open_in_memory().expect("open");
        let bl = db.create_blocklist("mixed", None, None).expect("bl");
        db.add_block_rule(Some(bl), None, "evil.com", true, "block")
            .expect("block");
        db.add_block_rule(Some(bl), None, "example.com", false, "allow")
            .expect("allow");
        assert_eq!(
            db.collect_active_block_rules().expect("active"),
            vec![block("evil.com", true)]
        );
    }

    #[test]
    fn import_blocklist_domains_skips_blanks_and_updates_the_timestamp() {
        let mut db = Db::open_in_memory().expect("open");
        let bl = db
            .create_blocklist("list", Some("https://x/list.txt"), Some("chk"))
            .expect("bl");
        let domains = vec![
            "  Example.com  ".to_string(),
            "a.example.com".to_string(),
            "".to_string(),
            "bad domain".to_string(),
            "a.example.com".to_string(), // duplicate in the same list: NOT EXISTS skips it
        ];
        let inserted = db
            .import_blocklist_domains(bl, &domains, "block", now())
            .expect("import");
        assert_eq!(
            inserted, 2,
            "example.com + a.example.com, dupes/blanks skipped"
        );

        let rules = db.list_block_rules().expect("rules");
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].domain, "example.com"); // ORDER BY id: first inserted
        assert_eq!(rules[0].action, "block");
        assert!(rules[0].include_subdomains);
        assert_eq!(rules[1].domain, "a.example.com");
        assert_eq!(rules[1].action, "block");

        let list = db.blocklist(bl).expect("get").expect("some");
        assert!(list.last_updated_utc.is_some());
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

    #[test]
    fn seed_default_blocklists_is_idempotent_and_creates_adult_and_social_rules() {
        let mut db = Db::open_in_memory().expect("open");
        db.seed_default_blocklists().expect("seed");

        let lists = db.list_blocklists().expect("lists");
        assert_eq!(lists.len(), 2);
        assert_eq!(lists[0].name, "Adult Content Filter");
        assert!(lists[0].enabled);
        assert_eq!(lists[1].name, "Social Media Filter");
        assert!(lists[1].enabled);

        let total_rules = DEFAULT_ADULT_DOMAINS.len() + DEFAULT_SOCIAL_MEDIA_DOMAINS.len();
        let rules = db.list_block_rules().expect("rules");
        assert_eq!(rules.len(), total_rules);
        assert!(rules.iter().all(|r| r.action == "block"));
        assert!(rules.iter().all(|r| r.include_subdomains));

        let active = db.collect_active_block_rules().expect("active");
        assert_eq!(active.len(), total_rules);

        // Second call must be idempotent
        db.seed_default_blocklists().expect("reseed");
        assert_eq!(db.list_blocklists().expect("lists").len(), 2);
        assert_eq!(db.list_block_rules().expect("rules").len(), total_rules);
    }
}
