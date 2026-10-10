//! Log directory resolution, tracing setup and the single-instance mutex name.

use super::*;

/// Kernel mutex name for the helper's single-instance guard.
///
/// `Local\` on purpose — the opposite choice from the agent: the helper is
/// one per LOGIN SESSION, because every desktop must sample its own
/// foreground window, so the guard must NOT span sessions. The username is
/// appended for fast user switching clarity; when it cannot be resolved the
/// bare name still guarantees one-helper-per-session, the invariant that
/// actually matters.
pub(crate) fn single_instance_mutex_name() -> String {
    single_instance_mutex_name_for(std::env::var("USERNAME").ok().as_deref())
}

/// Pure decision core of [`single_instance_mutex_name`].
pub(crate) fn single_instance_mutex_name_for(username: Option<&str>) -> String {
    match username {
        Some(user) if !user.is_empty() => format!(r"Local\screentime-session-{user}"),
        _ => r"Local\screentime-session".to_string(),
    }
}

/// Resolves the helper's log directory from the environment.
///
/// Rules, in priority order:
/// 1. `SCREENTIME_DATA_DIR` set → `<dir>/logs`. Dev parity: a developer
///    pointing the agent at a scratch tree gets the helper's logs in the same
///    place instead of scattered across user profiles.
/// 2. Otherwise `%LOCALAPPDATA%` → `<LOCALAPPDATA>/screentime/logs`. The
///    per-user location is correct in production because the helper runs
///    unprivileged inside a login session; ProgramData would invite
///    cross-user write contention.
/// 3. No usable `LOCALAPPDATA` (stripped-down contexts) → `./screentime-logs`
///    beside the working directory: last resort, still discoverable.
///
/// Resolution never fails outright — an unusable *directory* is handled by
/// init_tracing's console-only fallback instead of refusing to start.
pub(crate) fn session_log_dir() -> PathBuf {
    resolve_log_dir(
        std::env::var("SCREENTIME_DATA_DIR").ok().as_deref(),
        std::env::var("LOCALAPPDATA").ok().as_deref(),
    )
}

/// Pure decision core of [`session_log_dir`], parameterised so tests exercise
/// the rules without touching process-global environment state. Blank values
/// count as unset: an empty override silently resolving to a relative `logs`
/// folder would scatter files unpredictably.
pub(crate) fn resolve_log_dir(
    data_dir_env: Option<&str>,
    local_appdata_env: Option<&str>,
) -> PathBuf {
    let data_dir = data_dir_env.filter(|v| !v.is_empty());
    let local = local_appdata_env.filter(|v| !v.is_empty());
    match data_dir {
        Some(dir) => PathBuf::from(dir).join("logs"),
        None => match local {
            Some(base) => PathBuf::from(base).join("screentime").join("logs"),
            None => PathBuf::from("screentime-logs"),
        },
    }
}

/// Installs the session helper's tracing stack: one filter, two destinations.
///
/// Console output serves interactive development; a daily-rolling file serves
/// incidents — the helper is typically started detached from any console, so
/// stdout diagnostics would otherwise vanish entirely. Both layers share ONE
/// `RUST_LOG`-derived filter placed atop the registry stack so the file sees
/// exactly what the console would.
///
/// # The classic tracing-appender trap (why this returns a guard)
///
/// `non_blocking` hands back a [`WorkerGuard`] owning the background writer
/// thread; dropping it shuts that thread down and can lose buffered lines —
/// most commonly by letting it die inside the init function as a temporary.
/// `main` must therefore hold the return value for the whole process lifetime.
///
/// A broken log sink must never stop sampling: if the log directory cannot be
/// created or opened, this falls back to console-only logging with a loud
/// warning instead of failing startup.
pub(crate) fn init_tracing(log_dir: &Path) -> Option<WorkerGuard> {
    // Built once for both layers; RUST_LOG honoured, "info" when unset —
    // unchanged from the previous console-only setup.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    match build_daily_appender(log_dir, "screentime-session.log") {
        Ok(appender) => {
            // Non-blocking on purpose: a slow disk must never delay the 1 Hz
            // sampling/reporting cycle behind a logging syscall.
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let stdout_layer = tracing_subscriber::fmt::layer();
            let file_layer = tracing_subscriber::fmt::layer()
                // ANSI colour codes belong on terminals, not incident logs.
                .with_ansi(false)
                .with_writer(writer);
            tracing_subscriber::registry()
                .with(filter)
                .with(stdout_layer)
                .with(file_layer)
                .init();
            Some(guard)
        }
        Err(e) => {
            // Console-only fallback, identical to the pre-file-logging setup;
            // only after `.init()` does the warning below actually surface.
            tracing_subscriber::fmt().with_env_filter(filter).init();
            tracing::warn!(
                error = %e,
                dir = %log_dir.display(),
                "file logging unavailable; continuing with console output only"
            );
            None
        }
    }
}

/// Creates `log_dir` and opens a daily-rolling appender named `base_name`.
///
/// Uses the builder form deliberately: the `rolling::daily` convenience
/// constructor panics on an unusable directory, and startup must survive
/// that (see init_tracing's fallback contract).
pub(crate) fn build_daily_appender(
    log_dir: &Path,
    base_name: &str,
) -> anyhow::Result<tracing_appender::rolling::RollingFileAppender> {
    // Directory creation as its own step so the common failure (missing
    // parent) names the directory rather than surfacing as an open error.
    std::fs::create_dir_all(log_dir)
        .with_context(|| format!("creating log directory {}", log_dir.display()))?;
    tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        // The base name carries the extension so each day lands as
        // `screentime-session.log.<yyyy-mm-dd>`.
        .filename_prefix(base_name)
        .max_log_files(MAX_LOG_FILES)
        .build(log_dir)
        .with_context(|| {
            format!(
                "opening daily rolling log {base_name} in {}",
                log_dir.display()
            )
        })
}
