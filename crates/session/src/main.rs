//! `screentime-session` — the per-login-session helper.
//!
//! # Why this binary exists
//!
//! On Windows, a service running under `LocalSystem` lives in Session 0 and
//! cannot see the interactive desktop, so `GetForegroundWindow` returns nothing
//! useful and it cannot display UI (toasts, the block overlay). On Linux, the
//! compositor only speaks to clients inside the graphical session. Either way,
//! anything that needs the user's screen must live in a process that starts
//! *inside* the login session.
//!
//! This helper owns those responsibilities and forwards facts to the agent over
//! IPC: focused-window and idle state (M1), session lock/unlock events, toasts,
//! and the full-screen block overlay (M2).
//!
//! Currently a stub. Kept in the workspace so the build system, IPC contract
//! and packaging story are in place before the agent needs it.

use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("screentime-session stub — full implementation lands in M2");
    Ok(())
}
