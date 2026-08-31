//! Windows-service plumbing for the privileged agent (`--service`,
//! `--install`, `--uninstall`).
//!
//! # Why Session 0 is safe now
//!
//! Services run in Session 0 with no access to the user desktop, which would
//! have been fatal for the old in-agent sampling design. That coupling is
//! gone: the unprivileged session helper runs inside each login session,
//! samples the foreground window locally, and ships collapsed observations to
//! this process over `ReportUsage` on the named pipe. Everything this process
//! touches — SQLite, the limits engine, the pipe server — is desktop-free, so
//! running it as LocalSystem costs nothing. The GDI block overlay lives in the
//! session helper too, so blocking still reaches the user's screen even though
//! this process can never draw one itself.
//!
//! # Lifecycle (`sc start ScreentimeAgent`)
//!
//! The SCM spawns the binary → [`run_as_service`] enters the dispatch table →
//! [`win::service_entry`] runs on an SCM-owned thread. There we: register the
//! control handler FIRST (a stop arriving mid-startup must flip the shared
//! flag, not get answered by the SCM killing us), report Running, acquire the
//! single-instance guard, then execute exactly the console startup path
//! ([`crate::run_daemon`]). `SERVICE_CONTROL_STOP` turns the SAME shutdown
//! knob as Ctrl+C, so the loop exits through the identical graceful tail
//! (final-interval flush, clean logs); we then report StopPending → Stopped
//! with the run's outcome as the Win32 exit code.
//!
//! # Single instance vs the service controller
//!
//! The `Global\` mutex guard still runs before any real work. A duplicate
//! instance — console while a service is running, or vice versa — fails
//! cleanly: the service variant reports Stopped carrying the shared
//! already-running exit code instead of fighting over pipe and database.

// Only the non-Windows stubs below need bail/Context at this level; the
// Windows work lives in `mod win`, which owns its own imports.
#[cfg(windows)]
use anyhow::Result;
#[cfg(not(windows))]
use anyhow::{bail, Context, Result};

#[cfg(windows)]
pub(crate) fn run_as_service() -> Result<()> {
    win::run_as_service()
}

#[cfg(windows)]
pub(crate) fn install() -> Result<()> {
    win::install()
}

#[cfg(windows)]
pub(crate) fn uninstall() -> Result<()> {
    win::uninstall()
}

/// Non-Windows hosts build the same CLI surface but have no service
/// controller; fail loudly rather than silently doing nothing.
#[cfg(not(windows))]
pub(crate) fn run_as_service() -> Result<()> {
    bail!("--service is only supported on Windows")
}

#[cfg(not(windows))]
pub(crate) fn install() -> Result<()> {
    bail!("--install is only supported on Windows")
}

#[cfg(not(windows))]
pub(crate) fn uninstall() -> Result<()> {
    bail!("--uninstall is only supported on Windows")
}

// ---------------------------------------------------------------------------
// Windows implementation.
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod win {
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use std::time::Duration;

    use anyhow::{anyhow, bail, Context, Result};
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
    use windows_service::service_dispatcher;

    use crate::cli;
    use crate::{request_shutdown, run_daemon, try_acquire_single_instance, EXIT_ALREADY_RUNNING};

    /// How long we tell the SCM a stop may take. The graceful tail is two
    /// polling ticks plus one final DB write in the common case, so this is
    /// generous headroom, not a target.
    const STOP_WAIT_HINT: Duration = Duration::from_secs(10);

    /// Headroom for the startup work that happens AFTER we report Running
    /// (DB open/migrate, backend detection): normally well under a second,
    /// but a first-run migration must not look hung to the SCM.
    const START_WAIT_HINT: Duration = Duration::from_secs(30);

    /// Raw OS code for "this process was not started by the service
    /// controller". Hardcoded (instead of imported from the windows crate)
    /// because the agent otherwise never needs that feature gate.
    const ERROR_FAILED_SERVICE_CONTROLLER_CONNECT: i32 = 1053;

    // Entry handed to the SCM's dispatch table by `service_dispatcher::start`.
    // The macro generates the FFI shim converting the raw `(argc, argv)` pair
    // into owned `OsString`s.
    windows_service::define_windows_service!(ffi_service_main, service_entry);

    /// Runs on a thread owned by the SCM once the dispatcher accepts us.
    ///
    /// Errors cannot go through `main`'s anyhow printing here: stderr belongs
    /// to nobody under the SCM. They are surfaced via status reports (exit
    /// codes) plus best-effort tracing/eprintln.
    fn service_entry(_arguments: Vec<OsString>) {
        // First line the service instance ever writes: if a start ever times
        // out (event 7009), this line's presence — or absence — in
        // {data_dir}\logs tells you whether the binary reached its entrypoint
        // at all before SCM lost patience.
        tracing::info!("service entrypoint entered");
        if let Err(e) = service_body() {
            eprintln!("service error: {e:#}");
        }
    }

    fn service_status(
        state: ServiceState,
        controls: ServiceControlAccept,
        exit_code: ServiceExitCode,
        wait_hint: Duration,
    ) -> ServiceStatus {
        ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: controls,
            exit_code,
            checkpoint: 0,
            wait_hint,
            process_id: None,
        }
    }

    fn service_body() -> Result<()> {
        // Control handler FIRST: a stop during startup has to flip the shared
        // flag rather than race the initialization sequence.
        let status_handle =
            service_control_handler::register(cli::SERVICE_NAME, |control| match control {
                // Same one-flag path as the console ctrl handler; the main loop
                // polls it within a tick and shuts down gracefully.
                ServiceControl::Stop => {
                    request_shutdown();
                    ServiceControlHandlerResult::NoError
                }
                // All services must answer Interrogate, even trivially.
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            })
            .context("registering the service control handler")?;

        // Report Running up front per the spec'd lifecycle (leave StartPending
        // immediately), then execute THE console startup path unchanged.
        status_handle
            .set_service_status(service_status(
                ServiceState::Running,
                ServiceControlAccept::STOP,
                ServiceExitCode::Win32(0),
                START_WAIT_HINT,
            ))
            .context("reporting Running to the service controller")?;

        // Single-instance guard before any real work: a duplicate instance
        // (console already running, or a racing second start) fails cleanly
        // here instead of fighting over the pipe and database.
        let _single_instance = match try_acquire_single_instance() {
            Ok(guard) => guard,
            Err(st_win32::AlreadyRunning) => {
                finish_stopped(
                    &status_handle,
                    ServiceExitCode::Win32(EXIT_ALREADY_RUNNING as u32),
                );
                return Ok(());
            }
        };

        // The daemon owns tracing init, so the boot marker lands inside it
        // (mode = "service"); anything logged before that init is lost.
        let outcome = run_daemon("service");

        // Announce the stop window, then land the final state the way
        // `sc query` and event viewers expect: Win32(0) for a clean stop,
        // nonzero when the run itself failed.
        let _ = status_handle.set_service_status(service_status(
            ServiceState::StopPending,
            ServiceControlAccept::empty(),
            ServiceExitCode::Win32(0),
            STOP_WAIT_HINT,
        ));

        let exit_code = match &outcome {
            Ok(()) => ServiceExitCode::Win32(0),
            Err(e) => {
                tracing::error!(error = %e, "service run failed");
                ServiceExitCode::Win32(1)
            }
        };
        finish_stopped(&status_handle, exit_code);
        outcome
    }

    fn finish_stopped(
        status_handle: &windows_service::service_control_handler::ServiceStatusHandle,
        exit_code: ServiceExitCode,
    ) {
        // Nothing actionable if even this final report fails: the process is
        // exiting either way, and the SCM marks an unreported dead service
        // stopped on its own.
        let _ = status_handle.set_service_status(service_status(
            ServiceState::Stopped,
            ServiceControlAccept::empty(),
            exit_code,
            Duration::ZERO,
        ));
    }

    /// Block on the SCM dispatch table until the service stops; ServiceMain
    /// runs concurrently on the thread the controller spawned.
    pub(super) fn run_as_service() -> Result<()> {
        service_dispatcher::start(cli::SERVICE_NAME, ffi_service_main).map_err(|e| {
            // Launching `--service` by hand from a shell fails with
            // ERROR_FAILED_SERVICE_CONTROLLER_CONNECT: only the SCM may own a
            // service process. Say that plainly instead of leaking a raw 1053.
            let direct_launch = matches!(&e, windows_service::Error::Winapi(io)
                if io.raw_os_error() == Some(ERROR_FAILED_SERVICE_CONTROLLER_CONNECT));
            if direct_launch {
                anyhow!(
                    "--service must be started by the Windows service controller \
                     (sc.exe start {}); run with no arguments for console mode",
                    cli::SERVICE_NAME
                )
            } else {
                anyhow!(e).context("registering with the Windows service controller")
            }
        })
    }

    /// Register the service pointing at THIS executable, printing every
    /// command and its output so the operator can audit or replay them.
    pub(super) fn install() -> Result<()> {
        let exe = current_exe_for_sc()?;
        let created = run_sc(&cli::install_create_command(&exe))?;
        require_sc_success(&created)?;
        // sc.exe cannot carry "--service" inside binPath (single-token option
        // values), so the canonical ImagePath is written straight to the SCM
        // database afterwards; see cli::install_imagepath_command.
        let imagepath = run_sc(&cli::install_imagepath_command(&exe))?;
        require_sc_success(&imagepath)?;
        let described = run_sc(&cli::install_description_command())?;
        require_sc_success(&described)?;
        println!(
            "\ninstalled '{}'; start it with: sc.exe start {}",
            cli::SERVICE_DISPLAY_NAME,
            cli::SERVICE_NAME
        );
        Ok(())
    }

    /// Best-effort stop (not-running / not-installed are fine by design),
    /// then delete; deletion failure is fatal with the elevation hint.
    pub(super) fn uninstall() -> Result<()> {
        let stopped = run_sc(&cli::uninstall_stop_command())?;
        require_sc_success(&stopped).ok();
        let deleted = run_sc(&cli::uninstall_delete_command())?;
        require_sc_success(&deleted)?;
        println!("\nservice {} removed", cli::SERVICE_NAME);
        Ok(())
    }

    /// Path of this binary as sc.exe should store it.
    ///
    /// `current_exe()` can hand back a `\\?\`-prefixed verbatim path; the SCM
    /// stores ImagePath verbatim in the registry and tooling compares/displays
    /// it literally, so strip the prefix for a clean conventional value.
    fn current_exe_for_sc() -> Result<PathBuf> {
        let exe = std::env::current_exe().context("resolving the agent executable path")?;
        let text = exe.to_string_lossy();
        let plain = text.strip_prefix(r"\\?\").map(PathBuf::from).unwrap_or(exe);
        // 8.3 short form: a space-free binPath survives every quoting layer
        // (Rust argv escaping, cmd, PowerShell) without special handling. If
        // the volume has short-name generation disabled this fails loudly so
        // install.ps1's FSO-based fallback story applies instead of silently
        // registering an unstartable service.
        st_win32::short_path(&plain)
            .context("resolving the 8.3 short path (volume may have short names disabled)")
    }

    /// Run one `sc.exe` command line, echoing the command AND its output.
    ///
    /// The command lines are built from SHORT paths (see
    /// [`current_exe_for_sc`]) and therefore contain no quotes at all, so a
    /// direct spawn is safe: Rust passes each token verbatim and sc.exe sees
    /// exactly the documented `binPath= <path> --service` syntax. Do not
    /// reintroduce quoted long paths here — they were mangled by both the
    /// PowerShell and cmd quoting layers in the field.
    fn run_sc(command_line: &str) -> Result<std::process::Output> {
        println!("> {command_line}");
        let output = Command::new("cmd")
            .arg("/D")
            .arg("/C")
            .raw_arg(command_line)
            .output()
            .context("spawning cmd.exe to invoke sc.exe")?;
        print!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        Ok(output)
    }

    /// Map a failed sc.exe invocation onto the standard elevation hint.
    fn require_sc_success(output: &std::process::Output) -> Result<()> {
        if output.status.success() {
            return Ok(());
        }
        bail!(
            "sc.exe failed with exit code {:?}; the service database requires \
             administrator rights — run this command again from an elevated prompt",
            output.status.code()
        )
    }
}
