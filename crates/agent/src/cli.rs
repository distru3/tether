//! Hand-rolled command-line surface for the agent binary.
//!
//! # Why not a parsing crate
//!
//! The CLI has exactly four fixed verbs and no composition, so a dependency
//! would cost more than it saves; the entire grammar below fits on a screen
//! and is fully unit-testable as a pure function over the argument slice.
//!
//! # The verbs
//!
//! * *(no args)* — console mode: byte-for-byte the historical foreground
//!   daemon (`Ctrl+C` for graceful stop).
//! * `--service` — launched by the Windows service controller only; never run
//!   by hand from a terminal (the SCM must own the process).
//! * `--install` / `--uninstall` — one-shot `sc.exe` shims for registering or
//!   removing the service; they need an elevated prompt but do NOT start any
//!   daemon work.
//!
//! Everything here is deliberately free of OS calls so the decision table and
//! the generated `sc.exe` command lines can be tested on any host.

/// Kernel/service-controller name of the agent's Windows service. Referenced
/// consistently by the dispatcher, the status handler and both `sc.exe`
/// shims — change it here and nowhere else.
pub(crate) const SERVICE_NAME: &str = "ScreentimeAgent";

/// Friendly name shown in `services.msc`.
pub(crate) const SERVICE_DISPLAY_NAME: &str = "Screentime Agent";

/// One-line description attached to the service by `--install`.
pub(crate) const SERVICE_DESCRIPTION: &str =
    "Privileged screen-time tracker: owns the usage database and enforces app limits.";

/// What the parsed command line asks the process to do.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Foreground daemon, identical to pre-service behavior.
    Console,
    /// Run under the Windows service controller (`ServiceMain` path).
    RunAsService,
    /// Register the service via `sc.exe create` + description.
    Install,
    /// Stop (best-effort) and delete the service via `sc.exe`.
    Uninstall,
}

/// Decide what to do from the raw argument slice (argv[1..]).
///
/// Strict on purpose: exactly zero or one known flag is accepted; anything
/// else — unknown flags, extra positionals, duplicates — returns the usage
/// text so the caller can exit 1 with a self-explaining message.
pub(crate) fn decide(args: &[String]) -> Result<Action, String> {
    match args {
        [] => Ok(Action::Console),
        [flag] if flag == "--service" => Ok(Action::RunAsService),
        [flag] if flag == "--install" => Ok(Action::Install),
        [flag] if flag == "--uninstall" => Ok(Action::Uninstall),
        other => Err(format!(
            "error: unrecognized argument(s): {}\n\n{}",
            other.join(" "),
            usage_text()
        )),
    }
}

/// Usage text listing every verb; also the body of the unknown-flag error.
pub(crate) fn usage_text() -> String {
    // No interpolation, so a plain literal + to_string beats format! here.
    "usage: screentime-agent [FLAG]\n\
     \x20 (no args)     run the agent in console mode (foreground)\n\
     \x20 --service     run under the Windows service controller (launched by SCM)\n\
     \x20 --install     register the Windows service (elevated prompt required)\n\
     \x20 --uninstall   remove the Windows service (elevated prompt required)"
        .to_string()
}

/// The `sc.exe create` line `--install` runs.
///
/// `sc.exe` parses options with the value AFTER the space following `=`, and
/// each option value is a SINGLE token — a binPath of `<path> --service`
/// makes sc treat the flag as an unknown option (usage dump, exit 1639). So
/// creation registers the bare short path; [`install_imagepath_command`]
/// then writes the canonical ImagePath (path + flag) directly into the SCM
/// database.
///
/// `exe_path` MUST already be the 8.3 short form (see
/// `st_win32::short_path`): a space-free path needs no quotes, which keeps
/// this line byte-stable through every invoker (Rust argv escaping,
/// PowerShell native-arg passing, cmd multi-quote stripping — each has
/// mangled it in the field).
pub(crate) fn install_create_command(exe_path: &std::path::Path) -> String {
    format!(
        r#"sc.exe create {SERVICE_NAME} binPath= {} start= auto displayName= "{SERVICE_DISPLAY_NAME}""#,
        exe_path.display()
    )
}

/// The `reg add` line that writes the real ImagePath: short path plus
/// `--service`, quoted as one value. Exactly two quote characters, so even
/// the cmd layer preserves them verbatim.
pub(crate) fn install_imagepath_command(exe_path: &std::path::Path) -> String {
    format!(
        r#"reg add HKLM\SYSTEM\CurrentControlSet\Services\{SERVICE_NAME} /v ImagePath /t REG_EXPAND_SZ /d "{} --service" /f"#,
        exe_path.display()
    )
}

/// The `sc.exe description` line `--install` runs after creating the service.
pub(crate) fn install_description_command() -> String {
    format!(r#"sc.exe description {SERVICE_NAME} "{SERVICE_DESCRIPTION}""#)
}

/// Best-effort stop before deletion; failures are ignored by the caller
/// because the common cases (not running, already deleted) are fine here.
pub(crate) fn uninstall_stop_command() -> String {
    format!("sc.exe stop {SERVICE_NAME}")
}

/// The actual removal; its failure IS fatal for `--uninstall`.
pub(crate) fn uninstall_delete_command() -> String {
    format!("sc.exe delete {SERVICE_NAME}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_arguments_mean_console_mode() {
        assert_eq!(decide(&[]), Ok(Action::Console));
    }

    #[test]
    fn service_flag_selects_service_mode() {
        assert_eq!(decide(&args(&["--service"])), Ok(Action::RunAsService));
    }

    #[test]
    fn install_flag_selects_install() {
        assert_eq!(decide(&args(&["--install"])), Ok(Action::Install));
    }

    #[test]
    fn uninstall_flag_selects_uninstall() {
        assert_eq!(decide(&args(&["--uninstall"])), Ok(Action::Uninstall));
    }

    #[test]
    fn unknown_flag_is_rejected_with_usage_listing_all_verbs() {
        let err = decide(&args(&["--serv"])).expect_err("unknown flag");
        assert!(err.contains("--service"));
        assert!(err.contains("--install"));
        assert!(err.contains("--uninstall"));
        assert!(err.contains("(no args)"));
    }

    #[test]
    fn extra_arguments_beyond_a_known_flag_are_rejected() {
        // A second token would silently do nothing today; refuse instead of
        // guessing so scripts cannot drift into no-op invocations.
        assert!(decide(&args(&["--service", "--install"])).is_err());
        assert!(decide(&args(&["console", "extra"])).is_err());
    }

    #[test]
    fn create_command_uses_a_bare_short_path_without_the_flag() {
        let cmd = install_create_command(std::path::Path::new(
            r"C:\PROGRA~1\SCREEN~1\screentime-agent.exe",
        ));
        // Pinned exactly: bare short path, no --service (sc would reject it as
        // an unknown option); the flag lands via the ImagePath registry write.
        assert_eq!(
            cmd,
            r#"sc.exe create ScreentimeAgent binPath= C:\PROGRA~1\SCREEN~1\screentime-agent.exe start= auto displayName= "Screentime Agent""#
        );
    }

    #[test]
    fn imagepath_command_quotes_path_and_flag_as_one_registry_value() {
        let cmd = install_imagepath_command(std::path::Path::new(
            r"C:\PROGRA~1\SCREEN~1\screentime-agent.exe",
        ));
        assert_eq!(
            cmd,
            r#"reg add HKLM\SYSTEM\CurrentControlSet\Services\ScreentimeAgent /v ImagePath /t REG_EXPAND_SZ /d "C:\PROGRA~1\SCREEN~1\screentime-agent.exe --service" /f"#
        );
    }

    #[test]
    fn description_command_targets_the_agent_service_by_name() {
        let cmd = install_description_command();
        assert!(cmd.starts_with(&format!("sc.exe description {SERVICE_NAME} ")));
        // Quoted so multi-word descriptions survive sc.exe tokenization.
        assert!(cmd.contains(&format!("\"{SERVICE_DESCRIPTION}\"")));
    }

    #[test]
    fn uninstall_commands_stop_then_delete_the_same_service_name() {
        assert_eq!(
            uninstall_stop_command(),
            format!("sc.exe stop {SERVICE_NAME}")
        );
        assert_eq!(
            uninstall_delete_command(),
            format!("sc.exe delete {SERVICE_NAME}")
        );
    }
}
