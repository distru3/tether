//! Logon-autostart management for the session helper: a tiny hand-rolled CLI
//! (`screentime-session --autostart on|off|status`) plus the registry plumbing
//! it drives.
//!
//! # Why HKCU\Software\Microsoft\Windows\CurrentVersion\Run
//!
//! The Run key is deliberately unglamorous, and every alternative loses on at
//! least one axis that matters for *this* binary:
//!
//! - **Per-user by construction.** The helper must run inside each login
//!   session (Session-0 isolation makes service-side sampling useless), and
//!   HKCU gives exactly one autostart per logged-in user with zero extra
//!   machinery. A Startup-folder shortcut would do the same but cannot carry
//!   arguments cleanly and invites users to delete it casually; scheduled-task
//!   logon triggers can do it too but need elevation to manage reliably.
//! - **No elevation to write.** HKCU is writable by the plain user, so the
//!   installer script and even the user themselves can flip it without an
//!   admin token — unlike HKLM Run, services, or tasks.
//! - **Survives reboots, dies with the profile.** Entries fire on every
//!   interactive logon forever, yet are cleaned up automatically when the
//!   user profile is deleted; there is no orphaned machine-global state.
//!
//! # Why the value must be quoted
//!
//! Run values are executed roughly as a command line, so an unquoted path
//! containing spaces (this repo lives under OneDrive; installs land in
//! `C:\Program Files\Screentime`) parses as executable-plus-arguments and
//! silently launches nothing, or worse, the wrong thing. Wrapping the whole
//! path in double quotes is the standard cure. The flip side is that a path
//! which already *contains* a double quote can never be quoted safely, so we
//! refuse it up front rather than write an entry that breaks at next logon.
//!
//! # Layering
//!
//! Pure helpers ([`build_run_value`], [`exe_path_problem`], [`format_status`],
//! [`classify_cli`]) sit above the thin registry shell, mirroring the crate's
//! other modules: decisions are unit-testable without touching Windows, and
//! the OS surface stays small enough to eyeball. Non-Windows builds compile
//! everything and fail the actions honestly at runtime instead of pretending.
//!
//! The CLI runs *before* the single-instance guard and tracing init in
//! `main`: managing autostart must never collide with an already-running
//! helper (exit 2 "already running" would make `--autostart off` impossible
//! while the helper runs), and a CLI query has no business creating log files.

use anyhow::Context;

/// Subkey under `HKEY_CURRENT_USER` holding per-user logon programs.
///
/// Kept as a constant (not inline strings at call sites) so a test can pin
/// the exact contract the installer docs and the registry shell both rely on.
const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Value name inside [`RUN_SUBKEY`] reserved for this helper.
///
/// A stable, human-readable name: it is what shows up in tools like
/// Autoruns, and what `--autostart off` deletes. Renaming it would orphan
/// old entries, so treat it as API.
pub(crate) const VALUE_NAME: &str = "ScreentimeSession";

/// Exit status for CLI misuse and operational failure of autostart commands.
///
/// Distinct from the loop's `EXIT_ALREADY_RUNNING` (2) so scripts can tell
/// "your request failed" from "a helper is already running".
const EXIT_FAILURE: i32 = 1;

/// One-line usage shown whenever the argument vector cannot be understood.
const USAGE_LINE: &str = "usage: screentime-session [--autostart on|off|status]";

/// What the user asked the autostart CLI to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    /// Register (or overwrite) the Run entry pointing at this executable.
    On,
    /// Delete the Run entry if present; absence counts as success.
    Off,
    /// Report whether the entry exists, plus its stored path.
    Status,
}

/// Classifies the process argument vector into an autostart action.
///
/// Returns `Ok(None)` when the arguments say nothing about autostart — which
/// today means *no arguments at all*, the sampling loop's launch shape, whose
/// behaviour must remain byte-identical. Anything else that is not exactly
/// `--autostart <action>` is rejected rather than silently ignored: a typoed
/// flag (`--autostartt`) falling through to "start a second sampler" would
/// collide with the single-instance guard and confuse whoever scripted us.
pub(crate) fn classify_cli(args: &[String]) -> Result<Option<Action>, String> {
    match args {
        [] => Ok(None),
        [flag, action] if flag == "--autostart" => match action.as_str() {
            "on" => Ok(Some(Action::On)),
            "off" => Ok(Some(Action::Off)),
            "status" => Ok(Some(Action::Status)),
            other => Err(format!(
                "unknown --autostart action '{other}'; {USAGE_LINE}"
            )),
        },
        _ => Err(format!("unrecognised arguments; {USAGE_LINE}")),
    }
}

/// Builds the registry value body for `exe_path`.
///
/// The whole path is wrapped in literal double quotes because Run values are
/// interpreted as command lines (see module docs): `C:\My Dir\app.exe`
/// unquoted parses as program `C:\My` with argument `Dir\app.exe`.
pub(crate) fn build_run_value(exe_path: &str) -> String {
    format!("\"{exe_path}\"")
}

/// Rejects paths that can never be represented safely in a Run value.
///
/// Quoting guards against spaces, but a path containing a literal `"` breaks
/// out of the quotes we add, producing a command line nobody can predict.
/// Returning `Some(reason)` means "refuse"; `None` means safe to register.
pub(crate) fn exe_path_problem(exe_path: &str) -> Option<&'static str> {
    if exe_path.contains('"') {
        Some("executable path contains a double quote and cannot be registered safely")
    } else {
        None
    }
}

/// Human-readable status line for `--autostart status`.
///
/// `Some(stored)` renders as on-with-path (the path is what an operator needs
/// to spot a stale entry pointing at an uninstalled location); `None` renders
/// as plain off.
pub(crate) fn format_status(stored: Option<&str>) -> String {
    match stored {
        Some(path) => format!("autostart: on ({path})"),
        None => "autostart: off".to_string(),
    }
}

/// Entry point called from `main` before any startup machinery.
///
/// `Some(code)` means the arguments were an autostart request (or misuse of
/// them) and the process should exit with `code`; `None` means fall through
/// to the normal sampling loop untouched.
pub(crate) fn handle_cli(args: &[String]) -> Option<i32> {
    let action = match classify_cli(args) {
        Ok(Some(action)) => action,
        Ok(None) => return None,
        Err(message) => {
            eprintln!("error: {message}");
            return Some(EXIT_FAILURE);
        }
    };
    match run_action(action) {
        Ok(()) => Some(0),
        // Exactly one stderr line per failure: scripts capture it, humans read it.
        Err(e) => {
            eprintln!("error: {e:#}");
            Some(EXIT_FAILURE)
        }
    }
}

/// Performs one autostart action against the real registry.
pub(crate) fn run_action(action: Action) -> anyhow::Result<()> {
    match action {
        Action::On => enable(),
        Action::Off => disable(),
        Action::Status => show_status(),
    }
}

/// Registers the Run entry pointing at the currently running executable.
///
/// Resolves `current_exe` *at call time* so the entry always matches however
/// this binary was actually launched (dev tree, installed dir, renamed copy)
/// instead of baking in a compile-time guess.
fn enable() -> anyhow::Result<()> {
    let exe =
        std::env::current_exe().context("resolving the running executable's path (current_exe)")?;
    // Windows paths are UTF-16 natively, so lossy conversion only mangles
    // paths that were already unrepresentable in the registry.
    let exe_str = exe.to_string_lossy();
    if let Some(problem) = exe_path_problem(&exe_str) {
        anyhow::bail!("{problem}: {}", exe.display());
    }
    let value = build_run_value(&exe_str);
    set_run_value(&value)?;
    println!("{}", format_status(Some(&exe_str)));
    Ok(())
}

/// Deletes the Run entry; a missing key or value is idempotent success.
fn disable() -> anyhow::Result<()> {
    delete_run_value()?;
    println!("{}", format_status(None));
    Ok(())
}

/// Prints the current registration state.
fn show_status() -> anyhow::Result<()> {
    let stored = query_run_value()?;
    println!("{}", format_status(stored.as_deref()));
    Ok(())
}

// ---------------------------------------------------------------------------
// Registry shell (Windows). Three narrow operations — set, delete-if-present,
// read — each opening/closing the key around the call; no cached handles,
// because a CLI invocation performs exactly one operation and exits.
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod registry {
    use super::{RUN_SUBKEY, VALUE_NAME};
    use anyhow::{anyhow, Context};

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
        RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
        REG_VALUE_TYPE,
    };

    /// NUL-terminated UTF-16 for the wide registry APIs.
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Turns a nonzero `WIN32_ERROR` into a named, one-line error.
    fn expect_ok(
        api: &'static str,
        code: windows::Win32::Foundation::WIN32_ERROR,
    ) -> anyhow::Result<()> {
        if code == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(anyhow!("{api} failed (Win32 error {})", code.0))
        }
    }

    /// Creates-or-opens the Run key and writes `value` as REG_SZ.
    ///
    /// `RegCreateKeyW` over `RegOpenKeyExW` is what makes `--autostart on`
    /// idempotent: the key exists on every normal system, but a stripped or
    /// freshly-imaged profile may lack it, and create-or-open simply works.
    pub(super) fn set_run_value(value: &str) -> anyhow::Result<()> {
        unsafe {
            let subkey = wide(RUN_SUBKEY);
            let name = wide(VALUE_NAME);
            let mut key = HKEY::default();
            expect_ok(
                "RegCreateKeyW",
                RegCreateKeyW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()), &mut key),
            )
            .context("opening HKCU Run key for writing")?;

            // REG_SZ payload must include the terminating NUL in its byte count.
            let mut data = Vec::with_capacity((value.len() + 1) * 2);
            for unit in value.encode_utf16().chain(std::iter::once(0)) {
                data.extend_from_slice(&unit.to_le_bytes());
            }
            let result = expect_ok(
                "RegSetValueExW",
                RegSetValueExW(key, PCWSTR(name.as_ptr()), 0, REG_SZ, Some(&data)),
            )
            .context("writing the autostart value");
            let _ = RegCloseKey(key);
            result
        }
    }

    /// Deletes the value; returns whether anything was actually removed.
    ///
    /// Missing key/value are reported as `false` (nothing to do), not errors,
    /// so `--autostart off` succeeds identically on clean and dirty systems.
    pub(super) fn delete_run_value() -> anyhow::Result<bool> {
        unsafe {
            let subkey = wide(RUN_SUBKEY);
            let name = wide(VALUE_NAME);
            let mut key = HKEY::default();
            match RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                0,
                KEY_SET_VALUE,
                &mut key,
            ) {
                ERROR_SUCCESS => {}
                ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => return Ok(false),
                code => return expect_ok("RegOpenKeyExW", code).map(|()| false),
            }
            let result = match RegDeleteValueW(key, PCWSTR(name.as_ptr())) {
                ERROR_SUCCESS => Ok(true),
                ERROR_FILE_NOT_FOUND => Ok(false),
                code => expect_ok("RegDeleteValueW", code).map(|()| false),
            };
            let _ = RegCloseKey(key);
            result
        }
    }

    /// Reads the stored value, or `None` when absent.
    ///
    /// Two-step query (size probe, then fill) follows the documented pattern
    /// for value data whose length is unknown up front. Only REG_SZ is
    /// accepted: anything else means someone else wrote this value name, and
    /// reporting an error beats displaying binary junk as a "path".
    pub(super) fn query_run_value() -> anyhow::Result<Option<String>> {
        unsafe {
            let subkey = wide(RUN_SUBKEY);
            let name = wide(VALUE_NAME);
            let mut key = HKEY::default();
            match RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                0,
                KEY_QUERY_VALUE,
                &mut key,
            ) {
                ERROR_SUCCESS => {}
                ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => return Ok(None),
                code => return expect_ok("RegOpenKeyExW", code).map(|()| None),
            }

            let probe = (|| -> anyhow::Result<Option<String>> {
                let mut byte_len = 0u32;
                match RegQueryValueExW(
                    key,
                    PCWSTR(name.as_ptr()),
                    None,
                    None,
                    None,
                    Some(&mut byte_len),
                ) {
                    ERROR_SUCCESS => {}
                    // Key exists but our value does not: that IS "off".
                    ERROR_FILE_NOT_FOUND => return Ok(None),
                    code => return expect_ok("RegQueryValueExW", code).map(|()| None),
                }
                if byte_len == 0 {
                    // An empty value carries no path; treat as unregistered.
                    return Ok(None);
                }
                let mut data = vec![0u8; byte_len as usize];
                let mut kind = REG_VALUE_TYPE::default();
                expect_ok(
                    "RegQueryValueExW",
                    RegQueryValueExW(
                        key,
                        PCWSTR(name.as_ptr()),
                        None,
                        Some(&mut kind),
                        Some(data.as_mut_ptr()),
                        Some(&mut byte_len),
                    ),
                )
                .context("reading the autostart value")?;
                if kind != REG_SZ {
                    anyhow::bail!(
                        "autostart value has unexpected registry type {} (expected REG_SZ)",
                        kind.0
                    );
                }
                // The registry hands back little-endian UTF-16 bytes.
                let units: Vec<u16> = data[..byte_len as usize]
                    .chunks_exact(2)
                    .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                    .collect();
                let text = String::from_utf16_lossy(&units);
                Ok(Some(text.trim_end_matches('\0').to_string()))
            })();

            let _ = RegCloseKey(key);
            probe
        }
    }
}

#[cfg(windows)]
use registry::{delete_run_value, query_run_value, set_run_value};

/// Non-Windows stubs keep the crate compiling cross-platform (same policy as
/// the overlay shim) while failing the actions honestly: autostart is an
/// HKCU Run concept and simply does not exist elsewhere yet.
#[cfg(not(windows))]
fn set_run_value(_value: &str) -> anyhow::Result<()> {
    anyhow::bail!("autostart management is only implemented on Windows")
}

#[cfg(not(windows))]
fn delete_run_value() -> anyhow::Result<bool> {
    anyhow::bail!("autostart management is only implemented on Windows")
}

#[cfg(not(windows))]
fn query_run_value() -> anyhow::Result<Option<String>> {
    anyhow::bail!("autostart management is only implemented on Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_value_wraps_the_whole_exe_path_in_double_quotes() {
        assert_eq!(
            build_run_value(r"C:\Program Files\Screentime\screentime-session.exe"),
            "\"C:\\Program Files\\Screentime\\screentime-session.exe\""
        );
    }

    #[test]
    fn run_value_round_trips_back_to_the_original_path() {
        // Simulates how Windows consumes the value: strip the outer quotes and
        // the executable path must come back intact, spaces included.
        let exe = r"C:\Users\a k\OneDrive\Desktop\screentime\target\debug\screentime-session.exe";
        let value = build_run_value(exe);
        let trimmed = value.trim_matches('"');
        assert_eq!(trimmed, exe);
    }

    #[test]
    fn exe_paths_with_embedded_double_quotes_are_refused() {
        // r#"..."# because the path deliberately contains a raw double quote.
        assert!(exe_path_problem(r#"C:\we"ird\session.exe"#).is_some());
    }

    #[test]
    fn ordinary_paths_with_spaces_are_accepted_for_registration() {
        assert_eq!(
            exe_path_problem(r"C:\Program Files\Screentime\screentime-session.exe"),
            None
        );
    }

    #[test]
    fn no_arguments_means_the_normal_sampling_loop_should_run() {
        assert_eq!(classify_cli(&[]), Ok(None));
    }

    #[test]
    fn autostart_actions_are_recognised_verbatim() {
        assert_eq!(
            classify_cli(&["--autostart".into(), "on".into()]),
            Ok(Some(Action::On))
        );
        assert_eq!(
            classify_cli(&["--autostart".into(), "off".into()]),
            Ok(Some(Action::Off))
        );
        assert_eq!(
            classify_cli(&["--autostart".into(), "status".into()]),
            Ok(Some(Action::Status))
        );
    }

    #[test]
    fn an_unknown_autostart_action_is_rejected() {
        assert!(classify_cli(&["--autostart".into(), "restart".into()]).is_err());
    }

    #[test]
    fn a_dangling_autostart_flag_without_an_action_is_rejected() {
        assert!(classify_cli(&["--autostart".into()]).is_err());
    }

    #[test]
    fn stray_unrelated_arguments_are_rejected_rather_than_ignored() {
        assert!(classify_cli(&["--what-is-this".into()]).is_err());
        assert!(classify_cli(&["extra".into(), "args".into()]).is_err());
    }

    #[test]
    fn status_reporting_shows_on_with_the_stored_path() {
        assert_eq!(
            format_status(Some(r"D:\tools\screentime-session.exe")),
            "autostart: on (D:\\tools\\screentime-session.exe)"
        );
    }

    #[test]
    fn status_reporting_shows_off_when_nothing_is_registered() {
        assert_eq!(format_status(None), "autostart: off");
    }
}
