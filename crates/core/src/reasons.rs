//! What someone was about to do when an app was blocked, as answered on the
//! block screen ("What were you about to do?"). Stored one per app per day
//! and summed on the Activity page; recording one never changes enforcement.

/// The answers the block screen offers. Stored by [`BlockReason::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockReason {
    /// Wanted to finish what was in progress (a match, an episode).
    Finish,
    Bored,
    Habit,
}

impl BlockReason {
    pub const ALL: [BlockReason; 3] = [BlockReason::Finish, BlockReason::Bored, BlockReason::Habit];

    pub fn as_str(self) -> &'static str {
        match self {
            BlockReason::Finish => "finish",
            BlockReason::Bored => "bored",
            BlockReason::Habit => "habit",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == s)
    }
}

#[cfg(test)]
mod tests {
    use super::BlockReason;

    #[test]
    fn reasons_round_trip_and_unknown_ones_are_rejected() {
        for r in BlockReason::ALL {
            assert_eq!(BlockReason::parse(r.as_str()), Some(r));
        }
        assert_eq!(BlockReason::parse("Finish"), None);
        assert_eq!(BlockReason::parse(""), None);
        assert_eq!(BlockReason::parse("drop table"), None);
    }
}
