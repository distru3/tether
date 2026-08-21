//! `st-core` — portable domain logic for the screentime project.
//!
//! # Rules for this crate
//!
//! 1. **No operating-system APIs.** Not `windows`, not `x11rb`, not `nix`.
//!    Every OS interaction is expressed as a trait in [`platform`] and
//!    implemented in a `tracker-*` or `enforce-*` crate.
//! 2. **No I/O and no `std::time::SystemTime`.** Time comes from the [`clock`]
//!    module so that day-rollover, DST and clock-tamper logic is unit testable.
//! 3. **No SQL.** Persistence lives in `st-storage`; this crate defines the
//!    types and the rules that operate on them.
//!
//! Keeping these rules means that if the Linux port is cancelled we delete two
//! crates and a CI job, and nothing in here changes.

pub mod category;
pub mod clock;
pub mod daykey;
pub mod limits;
pub mod model;
pub mod platform;

pub use category::{BuiltinCategory, Category, CategoryKind, BUILTIN_CATEGORIES};
pub use clock::{Clock, ClockGuard, ClockVerdict, SystemClock};
pub use daykey::DayKey;
pub use limits::{Budget, Decision, Limit, LimitEngine, LimitTarget, UsageSnapshot};
pub use model::{AppKey, AppRecord, SubjectRef, UsageInterval};
pub use platform::{
    ActiveWindow, BlockRule, IdleMonitor, IdleState, NetworkFilter, PlatformError, PlatformResult,
    ProcessController, WindowTracker,
};
