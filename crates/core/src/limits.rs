//! The limits engine: given today's usage, decide whether an app may run.
//!
//! # Most-restrictive-wins
//!
//! Because an app can belong to several categories *and* carry its own limit
//! *and* be covered by the total daily budget, several limits can apply at once.
//! The engine always honours the tightest one. Anything else produces the
//! outcome nobody wants: a "30 minutes of social media" rule that is quietly
//! defeated by a looser per-app rule.

use serde::{Deserialize, Serialize};

use crate::model::{AppId, CategoryId};

/// What a limit applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LimitTarget {
    App(AppId),
    Category(CategoryId),
    /// Total screen time across everything blockable.
    Total,
}

/// A daily time budget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Limit {
    pub id: i64,
    pub target: LimitTarget,
    /// Budget used on days with no specific override.
    pub default_minutes: u32,
    /// Per-weekday overrides, Monday at index 0 through Sunday at index 6.
    /// `None` falls back to `default_minutes`.
    pub weekday_minutes: [Option<u32>; 7],
    pub enabled: bool,
}

impl Limit {
    pub fn new(id: i64, target: LimitTarget, default_minutes: u32) -> Self {
        Self {
            id,
            target,
            default_minutes,
            weekday_minutes: [None; 7],
            enabled: true,
        }
    }

    /// Budget in minutes for the given weekday (Monday = 0).
    pub fn minutes_for_weekday(&self, weekday: usize) -> u32 {
        self.weekday_minutes
            .get(weekday)
            .copied()
            .flatten()
            .unwrap_or(self.default_minutes)
    }

    pub fn allowed_secs_for_weekday(&self, weekday: usize) -> i64 {
        i64::from(self.minutes_for_weekday(weekday)) * 60
    }
}

use chrono::{DateTime, Utc};

/// Today's consumption, supplied by the storage layer.
///
/// A trait rather than a struct so that the engine stays free of SQL and can be
/// driven from a `HashMap` in tests.
pub trait UsageSnapshot {
    /// Seconds already spent against this target today.
    fn seconds_used(&self, target: &LimitTarget) -> i64;

    /// The absolute time at which the active wall-clock timer expires for this target.
    fn active_timer_expires_utc(&self, _target: &LimitTarget) -> Option<DateTime<Utc>> {
        None
    }
}

/// Remaining budget for one target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub target: LimitTarget,
    pub allowed_secs: i64,
    pub used_secs: i64,
}

impl Budget {
    pub fn remaining_secs(&self) -> i64 {
        self.allowed_secs - self.used_secs
    }

    pub fn exhausted(&self) -> bool {
        self.remaining_secs() <= 0
    }

    /// Fraction of the budget consumed, clamped to `0.0..=1.0`, for progress
    /// rings in the UI.
    pub fn fraction_used(&self) -> f64 {
        if self.allowed_secs <= 0 {
            return 1.0;
        }
        (self.used_secs as f64 / self.allowed_secs as f64).clamp(0.0, 1.0)
    }
}

/// The engine's verdict for one application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Keep running. `binding` is the tightest applicable limit, if any, so the
    /// tray can show the most urgent number.
    Allow {
        remaining_secs: i64,
        binding: Option<LimitTarget>,
    },
    /// Still allowed, but close enough to warn the user. Emitted once per
    /// threshold so the user gets a chance to wrap up rather than being cut off
    /// mid-sentence.
    Warn {
        remaining_secs: i64,
        binding: LimitTarget,
        threshold_secs: i64,
    },
    /// Budget spent. Freeze the process tree and show the block overlay until
    /// the day boundary.
    Block { binding: LimitTarget },
}

impl Decision {
    pub fn is_blocked(&self) -> bool {
        matches!(self, Decision::Block { .. })
    }

    pub fn remaining_secs(&self) -> Option<i64> {
        match self {
            Decision::Allow { remaining_secs, .. } | Decision::Warn { remaining_secs, .. } => {
                Some(*remaining_secs)
            }
            Decision::Block { .. } => Some(0),
        }
    }
}

/// Evaluates limits. Cheap to construct; rebuilt whenever limits change.
pub struct LimitEngine {
    limits: Vec<Limit>,
    /// Warning thresholds in seconds, kept sorted ascending.
    warn_at_secs: Vec<i64>,
}

impl LimitEngine {
    pub fn new(limits: Vec<Limit>, mut warn_at_secs: Vec<i64>) -> Self {
        warn_at_secs.sort_unstable();
        warn_at_secs.dedup();
        Self {
            limits,
            warn_at_secs,
        }
    }

    /// Sensible defaults: warn at 10 minutes, 5 minutes and 1 minute remaining.
    pub fn with_default_warnings(limits: Vec<Limit>) -> Self {
        Self::new(limits, vec![60, 300, 600])
    }

    pub fn limits(&self) -> &[Limit] {
        &self.limits
    }

    /// Every applicable budget for an app, tightest first. Used by the UI to
    /// explain *why* something is blocked.
    pub fn budgets_for(
        &self,
        app: AppId,
        categories: &[CategoryId],
        weekday: usize,
        usage: &dyn UsageSnapshot,
    ) -> Vec<Budget> {
        let mut budgets: Vec<Budget> = self
            .limits
            .iter()
            .filter(|l| l.enabled && applies_to(l, app, categories))
            .map(|l| Budget {
                target: l.target,
                allowed_secs: l.allowed_secs_for_weekday(weekday),
                used_secs: usage.seconds_used(&l.target),
            })
            .collect();
        budgets.sort_by_key(Budget::remaining_secs);
        budgets
    }

    /// Decide what to do about an app right now.
    ///
    /// `blockable` comes from the app's primary category
    /// ([`CategoryKind::NeverBlock`](crate::category::CategoryKind::NeverBlock)
    /// is never blocked, not even by the total daily budget — locking someone
    /// out of their terminal or file manager is never the right answer).
    pub fn evaluate(
        &self,
        app: AppId,
        categories: &[CategoryId],
        blockable: bool,
        weekday: usize,
        usage: &dyn UsageSnapshot,
        now: DateTime<Utc>,
    ) -> Decision {
        if !blockable {
            return Decision::Allow {
                remaining_secs: i64::MAX,
                binding: None,
            };
        }

        let tightest = self
            .budgets_for(app, categories, weekday, usage)
            .into_iter()
            .next();

        let Some(budget) = tightest else {
            return Decision::Allow {
                remaining_secs: i64::MAX,
                binding: None,
            };
        };

        let mut remaining = budget.remaining_secs();

        // If there's an active wall-clock timer, it unconditionally bypasses the budget
        // for the duration of the timer.
        if let Some(expires_utc) = usage.active_timer_expires_utc(&budget.target) {
            let timer_remaining = expires_utc.signed_duration_since(now).num_seconds();
            if timer_remaining > 0 {
                // If the timer gives more time than the budget, use it. If the budget
                // still has more time than the timer, the timer is basically redundant,
                // but we are in a timer state. The timer overrides the block.
                remaining = remaining.max(timer_remaining);
            }
        }

        if remaining <= 0 {
            return Decision::Block {
                binding: budget.target,
            };
        }

        // Smallest threshold that the remaining time has fallen below.
        if let Some(&threshold) = self.warn_at_secs.iter().find(|&&t| remaining <= t) {
            return Decision::Warn {
                remaining_secs: remaining,
                binding: budget.target,
                threshold_secs: threshold,
            };
        }

        Decision::Allow {
            remaining_secs: remaining,
            binding: Some(budget.target),
        }
    }
}

fn applies_to(limit: &Limit, app: AppId, categories: &[CategoryId]) -> bool {
    match limit.target {
        LimitTarget::Total => true,
        LimitTarget::App(id) => id == app,
        LimitTarget::Category(cid) => categories.contains(&cid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct FakeUsage {
        used: HashMap<LimitTarget, i64>,
        granted: HashMap<LimitTarget, i64>,
    }

    impl FakeUsage {
        fn used(mut self, target: LimitTarget, secs: i64) -> Self {
            self.used.insert(target, secs);
            self
        }
        fn granted(mut self, target: LimitTarget, secs: i64) -> Self {
            self.granted.insert(target, secs);
            self
        }
    }

    impl UsageSnapshot for FakeUsage {
        fn seconds_used(&self, target: &LimitTarget) -> i64 {
            self.used.get(target).copied().unwrap_or(0)
        }
        fn active_timer_expires_utc(&self, target: &LimitTarget) -> Option<DateTime<Utc>> {
            self.granted.get(target).map(|&secs| Utc::now() + chrono::Duration::seconds(secs))
        }
    }

    const TIKTOK: AppId = 1;
    const SOCIAL: CategoryId = 10;
    const SHORTFORM: CategoryId = 11;
    const MONDAY: usize = 0;
    const SUNDAY: usize = 6;

    #[test]
    fn no_limits_means_unrestricted() {
        let engine = LimitEngine::with_default_warnings(vec![]);
        let usage = FakeUsage::default();
        assert!(matches!(
            engine.evaluate(TIKTOK, &[SOCIAL], true, MONDAY, &usage, chrono::Utc::now()),
            Decision::Allow { binding: None, .. }
        ));
    }

    #[test]
    fn exhausted_budget_blocks() {
        let engine = LimitEngine::with_default_warnings(vec![Limit::new(
            1,
            LimitTarget::Category(SOCIAL),
            30,
        )]);
        let usage = FakeUsage::default().used(LimitTarget::Category(SOCIAL), 30 * 60);

        assert_eq!(
            engine.evaluate(TIKTOK, &[SOCIAL], true, MONDAY, &usage, chrono::Utc::now()),
            Decision::Block {
                binding: LimitTarget::Category(SOCIAL)
            }
        );
    }

    #[test]
    fn tightest_limit_wins_across_overlapping_categories() {
        // TikTok is Social Media (60 min) and Short-Form Video (15 min).
        // The 15-minute rule must win.
        let engine = LimitEngine::with_default_warnings(vec![
            Limit::new(1, LimitTarget::Category(SOCIAL), 60),
            Limit::new(2, LimitTarget::Category(SHORTFORM), 15),
        ]);
        let usage = FakeUsage::default()
            .used(LimitTarget::Category(SOCIAL), 16 * 60)
            .used(LimitTarget::Category(SHORTFORM), 16 * 60);

        assert_eq!(
            engine.evaluate(TIKTOK, &[SOCIAL, SHORTFORM], true, MONDAY, &usage, chrono::Utc::now()),
            Decision::Block {
                binding: LimitTarget::Category(SHORTFORM)
            }
        );
    }

    #[test]
    fn a_loose_per_app_limit_cannot_defeat_a_tight_category_limit() {
        let engine = LimitEngine::with_default_warnings(vec![
            Limit::new(1, LimitTarget::App(TIKTOK), 240),
            Limit::new(2, LimitTarget::Category(SHORTFORM), 15),
        ]);
        let usage = FakeUsage::default().used(LimitTarget::Category(SHORTFORM), 20 * 60);

        assert!(engine
            .evaluate(TIKTOK, &[SHORTFORM], true, MONDAY, &usage, chrono::Utc::now())
            .is_blocked());
    }

    #[test]
    fn total_budget_applies_to_every_blockable_app() {
        let engine =
            LimitEngine::with_default_warnings(vec![Limit::new(1, LimitTarget::Total, 120)]);
        let usage = FakeUsage::default().used(LimitTarget::Total, 120 * 60);

        assert_eq!(
            engine.evaluate(999, &[SOCIAL], true, MONDAY, &usage, chrono::Utc::now()),
            Decision::Block {
                binding: LimitTarget::Total
            }
        );
    }

    #[test]
    fn never_blockable_apps_survive_an_exhausted_total_budget() {
        let engine =
            LimitEngine::with_default_warnings(vec![Limit::new(1, LimitTarget::Total, 120)]);
        let usage = FakeUsage::default().used(LimitTarget::Total, 500 * 60);

        // e.g. the terminal, the file manager, or this app itself.
        assert!(!engine.evaluate(42, &[], false, MONDAY, &usage, chrono::Utc::now()).is_blocked());
    }

    #[test]
    fn warns_at_the_tightest_threshold_crossed() {
        let engine = LimitEngine::with_default_warnings(vec![Limit::new(
            1,
            LimitTarget::Category(SOCIAL),
            30,
        )]);

        // 4 minutes left -> the 5-minute warning, not the 10-minute one.
        let usage = FakeUsage::default().used(LimitTarget::Category(SOCIAL), 26 * 60);
        match engine.evaluate(TIKTOK, &[SOCIAL], true, MONDAY, &usage, chrono::Utc::now()) {
            Decision::Warn {
                threshold_secs,
                remaining_secs,
                ..
            } => {
                assert_eq!(threshold_secs, 300);
                assert_eq!(remaining_secs, 4 * 60);
            }
            other => panic!("expected Warn, got {other:?}"),
        }

        // 30 seconds left -> the 1-minute warning.
        let usage = FakeUsage::default().used(LimitTarget::Category(SOCIAL), 29 * 60 + 30);
        match engine.evaluate(TIKTOK, &[SOCIAL], true, MONDAY, &usage, chrono::Utc::now()) {
            Decision::Warn { threshold_secs, .. } => assert_eq!(threshold_secs, 60),
            other => panic!("expected Warn, got {other:?}"),
        }
    }

    #[test]
    fn weekday_overrides_replace_the_default() {
        let mut limit = Limit::new(1, LimitTarget::Category(SOCIAL), 30);
        limit.weekday_minutes[SUNDAY] = Some(120);
        let engine = LimitEngine::with_default_warnings(vec![limit]);
        let usage = FakeUsage::default().used(LimitTarget::Category(SOCIAL), 45 * 60);

        // Blocked on a Monday...
        assert!(engine
            .evaluate(TIKTOK, &[SOCIAL], true, MONDAY, &usage, chrono::Utc::now())
            .is_blocked());
        // ...but fine on a Sunday.
        assert!(!engine
            .evaluate(TIKTOK, &[SOCIAL], true, SUNDAY, &usage, chrono::Utc::now())
            .is_blocked());
    }

    #[test]
    fn an_override_grant_unblocks_until_it_is_consumed() {
        let engine = LimitEngine::with_default_warnings(vec![Limit::new(
            1,
            LimitTarget::Category(SOCIAL),
            30,
        )]);
        let target = LimitTarget::Category(SOCIAL);

        let usage = FakeUsage::default().used(target, 31 * 60);
        assert!(engine
            .evaluate(TIKTOK, &[SOCIAL], true, MONDAY, &usage, chrono::Utc::now())
            .is_blocked());

        // "+15 minutes", PIN approved.
        let usage = FakeUsage::default()
            .used(target, 31 * 60)
            .granted(target, 15 * 60);
        assert!(!engine
            .evaluate(TIKTOK, &[SOCIAL], true, MONDAY, &usage, chrono::Utc::now())
            .is_blocked());
    }

    #[test]
    fn disabled_limits_are_ignored() {
        let mut limit = Limit::new(1, LimitTarget::Category(SOCIAL), 1);
        limit.enabled = false;
        let engine = LimitEngine::with_default_warnings(vec![limit]);
        let usage = FakeUsage::default().used(LimitTarget::Category(SOCIAL), 99 * 60);

        assert!(!engine
            .evaluate(TIKTOK, &[SOCIAL], true, MONDAY, &usage, chrono::Utc::now())
            .is_blocked());
    }

    #[test]
    fn a_zero_minute_limit_blocks_immediately() {
        let engine = LimitEngine::with_default_warnings(vec![Limit::new(
            1,
            LimitTarget::Category(SOCIAL),
            0,
        )]);
        assert!(engine
            .evaluate(TIKTOK, &[SOCIAL], true, MONDAY, &FakeUsage::default(), chrono::Utc::now())
            .is_blocked());
    }

    #[test]
    fn budgets_are_reported_tightest_first() {
        let engine = LimitEngine::with_default_warnings(vec![
            Limit::new(1, LimitTarget::Category(SOCIAL), 60),
            Limit::new(2, LimitTarget::Category(SHORTFORM), 15),
            Limit::new(3, LimitTarget::Total, 300),
        ]);
        let budgets =
            engine.budgets_for(TIKTOK, &[SOCIAL, SHORTFORM], MONDAY, &FakeUsage::default());

        assert_eq!(budgets.len(), 3);
        assert_eq!(budgets[0].target, LimitTarget::Category(SHORTFORM));
        assert_eq!(budgets[2].target, LimitTarget::Total);
    }
}
