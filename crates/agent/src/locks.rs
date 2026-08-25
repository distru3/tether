//! One poisoning story for every mutex in the agent.
//!
//! A poisoned `std::sync::Mutex` means some thread panicked *while holding*
//! the lock. Historically the IPC server degraded gracefully (returned an
//! error to that one client) while the main loop `.unwrap()`ed the same lock
//! about ten times, so one panic anywhere killed the whole daemon — the worst
//! of both worlds.
//!
//! The unified policy: **recover and keep serving.** A panic mid-request may
//! leave that request's writes partial, but SQLite transactions either commit
//! or roll back atomically, so the database itself stays consistent; the
//! alternative — dying — converts one bad request into total loss of
//! enforcement, which for a parental-control daemon is strictly worse than a
//! partially applied limit edit. Every lock site in the agent goes through
//! [`lock_recover`] / [`lock_db`] so the policy lives in exactly one place.

use std::sync::{Mutex, MutexGuard};

use st_storage::Db;

/// Lock `mutex`, recovering from poisoning instead of panicking or degrading.
///
/// Logs at `error` level: a poison is a bug and must be loud, but it must not
/// take the daemon down.
pub(crate) fn lock_recover<'a, T>(mutex: &'a Mutex<T>, what: &'static str) -> MutexGuard<'a, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        tracing::error!(
            what,
            "mutex poisoned; recovering — the panicking thread's writes may be partial"
        );
        poisoned.into_inner()
    })
}

/// [`lock_recover`] specialised to the database mutex, the common case.
pub(crate) fn lock_db(db: &Mutex<Db>) -> MutexGuard<'_, Db> {
    lock_recover(db, "database")
}
