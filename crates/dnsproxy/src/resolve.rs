//! Pure resolve decision: is a domain blocked or forwarded?
//!
//! This is the ONLY logic the DNS responder consults, and it is deliberately
//! free of OS calls, sockets and wall-clock reads so it can be unit-tested
//! exhaustively without a listener. The allow-override resolution happens one
//! layer up (in `st-storage::collect_active_block_rules`), which means the rule
//! slice handed in here is already the effective set: it contains only rules
//! the policy engine decided should *block*. Contradictory `allow` rules never
//! reach this function, so "blocked if any rule matches" is a complete and
//! correct answer.
//!
//! # Matching semantics (exact, shared with the storage allow-resolver)
//!
//! A rule on domain `D` with `include_subdomains = true` matches:
//! * the exact name `D`, and
//! * every name ending in `.D` (any number of labels to the left).
//!
//! With `include_subdomains = false` it matches only the exact name `D`.
//! A blank `D`, or one that normalises to `.`/`..`, never matches anything.
//! Names are compared case-insensitively after lower-casing; a single trailing
//! dot (the FQDN form) and any leading dot are stripped, so `example.com.`
//! matches a rule on `example.com`.
//!
//! Matching never consults the parent-name-first rule the way a public suffix
//! table would — the rules are authoritative, and the list is already
//! allow-resolved, so ordering is irrelevant.

use st_core::platform::BlockRule;

/// What the responder should do with a query name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// The name is covered by an effective block rule: reply `NXDOMAIN`.
    Block,
    /// Not covered by any rule: forward to the upstream resolver.
    Forward,
}

/// Decide whether `domain` is blocked under the effective rule set.
///
/// `domain` may be a bare hostname or an FQDN with a trailing dot; it is
/// lower-cased and trimmed here. This is the same normalisation the storage
/// allow-resolver applies, so the two never disagree about a name's shape.
pub fn decision(domain: &str, rules: &[BlockRule]) -> Decision {
    let name = match normalize(domain) {
        Some(name) => name,
        None => return Decision::Forward,
    };

    for rule in rules {
        let Some(root) = normalize(&rule.domain) else {
            continue;
        };
        if matches_root(&name, &root, rule.include_subdomains) {
            return Decision::Block;
        }
    }
    Decision::Forward
}

/// Lower-case and strip one leading + one trailing dot. Returns `None` when
/// nothing usable remains (blank, or just dots).
fn normalize(name: &str) -> Option<String> {
    let mut s = name.trim().to_ascii_lowercase();
    if s.starts_with('.') {
        s.remove(0);
    }
    while s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() {
        return None;
    }
    // Reject internal empty labels (`a..b`, `a.`) so malformed names never
    // match a rule they should not — a query name is already well-formed by the
    // time it reaches us, but a unit test or a future caller may not be.
    if s.contains("..") || s.starts_with('.') || s.ends_with('.') {
        return None;
    }
    Some(s)
}

/// Does the (normalised) name fall under the (normalised) `root`?
fn matches_root(name: &str, root: &str, include_subdomains: bool) -> bool {
    if name == root {
        return true;
    }
    include_subdomains
        && name.ends_with(root)
        && name.len() > root.len()
        && name.as_bytes()[name.len() - root.len() - 1] == b'.'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(domain: &str, include_subdomains: bool) -> BlockRule {
        BlockRule {
            domain: domain.into(),
            include_subdomains,
        }
    }

    #[test]
    fn exact_host_match_blocks() {
        let rules = [rule("example.com", false)];
        assert_eq!(decision("example.com", &rules), Decision::Block);
    }

    #[test]
    fn non_covered_host_forwards() {
        let rules = [rule("example.com", false)];
        assert_eq!(decision("other.com", &rules), Decision::Forward);
    }

    #[test]
    fn wildcard_covers_a_subdomain() {
        let rules = [rule("example.com", true)];
        assert_eq!(decision("a.example.com", &rules), Decision::Block);
    }

    #[test]
    fn exact_host_does_not_cover_subdomains() {
        let rules = [rule("example.com", false)];
        assert_eq!(decision("a.example.com", &rules), Decision::Forward);
    }

    #[test]
    fn wildcard_covers_deeply_nested_labels() {
        let rules = [rule("example.com", true)];
        assert_eq!(decision("x.y.z.example.com", &rules), Decision::Block);
    }

    #[test]
    fn wildcard_covers_the_root_itself() {
        let rules = [rule("example.com", true)];
        assert_eq!(decision("example.com", &rules), Decision::Block);
    }

    #[test]
    fn a_sibling_is_not_covered() {
        let rules = [rule("example.com", true)];
        assert_eq!(decision("badexample.com", &rules), Decision::Forward);
        assert_eq!(decision("x.com", &rules), Decision::Forward);
    }

    #[test]
    fn matching_is_case_insensitive() {
        let rules = [rule("Example.COM", true)];
        assert_eq!(decision("SUB.Example.com", &rules), Decision::Block);
        assert_eq!(decision("example.com", &rules), Decision::Block);
    }

    #[test]
    fn trailing_dot_is_stripped_in_query_and_rule() {
        let rules = [rule("example.com", true)];
        assert_eq!(decision("a.example.com.", &rules), Decision::Block);
        assert_eq!(decision("example.com.", &rules), Decision::Block);
    }

    #[test]
    fn leading_dot_is_stripped_in_the_rule() {
        let rules = [rule(".example.com", true)];
        assert_eq!(decision("a.example.com", &rules), Decision::Block);
    }

    #[test]
    fn empty_rules_forward_everything() {
        assert_eq!(decision("example.com", &[]), Decision::Forward);
    }

    #[test]
    fn blank_rule_domains_never_match() {
        let rules = [rule("", true), rule("   ", false), rule(".", true)];
        assert_eq!(decision("example.com", &rules), Decision::Forward);
        assert_eq!(decision("", &rules), Decision::Forward);
    }

    #[test]
    fn malformed_query_names_never_block() {
        // Internal empty label: cannot be a real query, must not match rules.
        let rules = [rule("com", true)];
        assert_eq!(decision("a..b.com", &rules), Decision::Forward);
    }

    #[test]
    fn multi_label_root_matches_only_its_own_suffix() {
        let rules = [rule("foo.bar.com", true)];
        assert_eq!(decision("x.foo.bar.com", &rules), Decision::Block);
        assert_eq!(decision("bar.com", &rules), Decision::Forward);
        assert_eq!(decision("foo.com", &rules), Decision::Forward);
    }

    #[test]
    fn a_cross_domain_suffix_trick_is_ignored() {
        // "notexample.com" must not be caught by a rule on "example.com".
        let rules = [rule("example.com", true)];
        assert_eq!(decision("notexample.com", &rules), Decision::Forward);
    }

    #[test]
    fn rule_list_is_ordering_independent() {
        let a = [rule("example.com", true), rule("a.example.com", false)];
        let b = [rule("a.example.com", false), rule("example.com", true)];
        // a.example.com is blocked by the wildcard regardless of position.
        assert_eq!(decision("a.example.com", &a), Decision::Block);
        assert_eq!(decision("a.example.com", &b), Decision::Block);
    }
}
