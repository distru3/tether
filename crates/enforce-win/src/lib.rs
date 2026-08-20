//! Windows enforcement: freezing applications and filtering domains.
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
