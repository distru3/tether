//! Windows enforcement: closing a blocked app on request and filtering
//! domains through the hosts file. Blocking itself is the block screen, drawn
//! by the session helper and the UI.
//!
//! On non-Windows targets this crate compiles to nothing.

#[cfg(windows)]
mod hosts;
#[cfg(windows)]
mod process;

#[cfg(windows)]
pub use hosts::HostsFileFilter;
#[cfg(windows)]
pub use process::Win32ProcessController;
