//! Categories: the built-in taxonomy plus the rules about what may be blocked.

use serde::{Deserialize, Serialize};

use crate::model::CategoryId;

/// Governs what the UI will *let* the user do with a category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CategoryKind {
    /// Normal category: can carry a daily time budget and be blocked.
    Limitable,
    /// Always-on content filter with no time budget. Offering "2 hours of
    /// gambling per day" would be absurd, so the UI only exposes an on/off
    /// switch for these.
    BlockOnly,
    /// Tracked and reported, but never blockable.
    ///
    /// This is a safety rail, not a limitation: blocking your terminal, file
    /// manager or the screentime app itself is how a self-control tool turns
    /// into an unusable machine and a support ticket.
    NeverBlock,
}

/// A category as shipped with the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinCategory {
    /// Stable identifier. Never change these: user limits reference them and
    /// the signature database keys off them.
    pub slug: &'static str,
    /// Default English display name; localised in the UI layer.
    pub name: &'static str,
    pub kind: CategoryKind,
    /// Default accent colour, as a hex string.
    pub color: &'static str,
}

/// A category as stored, including user-created ones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub id: CategoryId,
    pub slug: String,
    pub name: String,
    pub kind: CategoryKind,
    pub color: String,
    pub builtin: bool,
}

/// The shipped taxonomy.
///
/// Social Media and Short-Form Video are deliberately separate even though they
/// overlap heavily, because "cut my doomscrolling but keep messaging" is the
/// single most common thing people actually want. The overlap is handled by an
/// app having one primary category and any number of tags.
pub const BUILTIN_CATEGORIES: &[BuiltinCategory] = &[
    BuiltinCategory { slug: "games",            name: "Games",                  kind: CategoryKind::Limitable, color: "#8b5cf6" },
    BuiltinCategory { slug: "social-media",     name: "Social Media",           kind: CategoryKind::Limitable, color: "#3b82f6" },
    BuiltinCategory { slug: "short-form-video", name: "Short-Form Video",       kind: CategoryKind::Limitable, color: "#ec4899" },
    BuiltinCategory { slug: "video-streaming",  name: "Video & Streaming",      kind: CategoryKind::Limitable, color: "#ef4444" },
    BuiltinCategory { slug: "music-audio",      name: "Music & Audio",          kind: CategoryKind::Limitable, color: "#22c55e" },
    BuiltinCategory { slug: "news",             name: "News",                   kind: CategoryKind::Limitable, color: "#f97316" },
    BuiltinCategory { slug: "shopping",         name: "Shopping",               kind: CategoryKind::Limitable, color: "#eab308" },
    BuiltinCategory { slug: "communication",    name: "Communication",          kind: CategoryKind::Limitable, color: "#06b6d4" },
    BuiltinCategory { slug: "productivity",     name: "Productivity & Office",  kind: CategoryKind::Limitable, color: "#0ea5e9" },
    BuiltinCategory { slug: "creativity",       name: "Creativity & Design",    kind: CategoryKind::Limitable, color: "#a855f7" },
    BuiltinCategory { slug: "education",        name: "Education & Reading",    kind: CategoryKind::Limitable, color: "#14b8a6" },
    BuiltinCategory { slug: "finance",          name: "Finance",                kind: CategoryKind::Limitable, color: "#65a30d" },
    BuiltinCategory { slug: "ai-chatbots",      name: "AI Assistants",          kind: CategoryKind::Limitable, color: "#7c3aed" },
    BuiltinCategory { slug: "uncategorized",    name: "Uncategorized",          kind: CategoryKind::Limitable, color: "#94a3b8" },
    // Deliberately never blockable.
    BuiltinCategory { slug: "development",      name: "Development & Tools",    kind: CategoryKind::NeverBlock, color: "#64748b" },
    BuiltinCategory { slug: "utilities-system", name: "Utilities & System",     kind: CategoryKind::NeverBlock, color: "#475569" },
    // Filter-only.
    BuiltinCategory { slug: "adult-content",    name: "Adult Content",          kind: CategoryKind::BlockOnly, color: "#be123c" },
    BuiltinCategory { slug: "gambling",         name: "Gambling & Betting",     kind: CategoryKind::BlockOnly, color: "#9f1239" },
];

/// Slug assigned to anything the classifier cannot place.
pub const UNCATEGORIZED_SLUG: &str = "uncategorized";

pub fn builtin_by_slug(slug: &str) -> Option<&'static BuiltinCategory> {
    BUILTIN_CATEGORIES.iter().find(|c| c.slug == slug)
}

impl Category {
    /// Whether a daily time budget may be attached to this category.
    pub fn accepts_time_limit(&self) -> bool {
        matches!(self.kind, CategoryKind::Limitable)
    }

    /// Whether members of this category may ever be blocked.
    pub fn blockable(&self) -> bool {
        matches!(self.kind, CategoryKind::Limitable | CategoryKind::BlockOnly)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn slugs_are_unique() {
        let mut seen = HashSet::new();
        for c in BUILTIN_CATEGORIES {
            assert!(seen.insert(c.slug), "duplicate slug: {}", c.slug);
        }
    }

    #[test]
    fn uncategorized_exists_because_the_classifier_depends_on_it() {
        assert!(builtin_by_slug(UNCATEGORIZED_SLUG).is_some());
    }

    #[test]
    fn system_and_dev_categories_cannot_be_blocked() {
        for slug in ["development", "utilities-system"] {
            let c = builtin_by_slug(slug).expect("category present");
            assert_eq!(c.kind, CategoryKind::NeverBlock, "{slug} must not be blockable");
        }
    }

    #[test]
    fn block_only_categories_reject_time_budgets() {
        for slug in ["adult-content", "gambling"] {
            let c = builtin_by_slug(slug).expect("category present");
            assert_eq!(c.kind, CategoryKind::BlockOnly);
        }
    }

    #[test]
    fn colors_are_hex() {
        for c in BUILTIN_CATEGORIES {
            assert!(c.color.starts_with('#') && c.color.len() == 7, "bad color on {}", c.slug);
        }
    }
}
