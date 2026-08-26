//! Shared Win32 plumbing for the screentime workspace.
//!
//! # Why this crate exists
//!
//! `tracker-win`, `enforce-win` and `session` each carried a private copy of
//! the same three-step dance: `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)`,
//! a wide-string buffer fed to `QueryFullProcessImageNameW`, and lossy UTF-16
//! decoding. Copy-pasted FFI drifts, so this crate is the single canonical
//! implementation: fixes and audits land in one place. Existing crates adopt
//! it later; nothing here depends on them.

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::{
    CreateMutexW, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};

/// Buffer ceiling for `QueryFullProcessImageNameW`, in UTF-16 code units.
///
/// Long paths routinely exceed `MAX_PATH`; 32k wide chars is the documented
/// maximum NT path length, so one fixed allocation always suffices and no
/// retry loop is needed. Matches what the three origin crates used.
const IMAGE_PATH_MAX_WCHARS: usize = 32_768;

/// An owning wrapper around a Win32 process handle that calls `CloseHandle`
/// on drop.
///
/// Why RAII: every original call site had to remember `CloseHandle` on each
/// early-return path; one forgotten branch leaks a kernel handle forever (the
/// tracker polls many times per minute). This type makes the close automatic
/// and unconditional.
#[derive(Debug)]
pub struct OwnedProcessHandle {
    handle: HANDLE,
}

impl OwnedProcessHandle {
    /// Borrows the raw handle for passing to other Win32 calls.
    pub fn as_raw(&self) -> HANDLE {
        self.handle
    }

    /// Relinquishes ownership without closing the handle.
    ///
    /// The caller becomes responsible for calling `CloseHandle` exactly once.
    pub fn into_raw(self) -> HANDLE {
        let handle = self.handle;
        std::mem::forget(self);
        handle
    }

    /// Wraps a raw process handle in RAII ownership.
    ///
    /// # Safety
    ///
    /// `handle` must have been obtained from a successful call such as
    /// `OpenProcess` (or be null) and must not be owned anywhere else;
    /// otherwise it will be closed twice.
    pub unsafe fn from_raw(handle: HANDLE) -> Self {
        Self { handle }
    }
}

impl Drop for OwnedProcessHandle {
    fn drop(&mut self) {
        // SAFETY: self.handle came from OpenProcess (see from_raw's contract),
        // so closing it here exactly once is correct. A CloseHandle failure
        // has no useful recovery during drop, so the error is ignored — same
        // as every origin call site did.
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

// ---------------------------------------------------------------------------
// Single-instance guards.
//
// Why a named kernel mutex rather than a lock file: a lock file can be left
// behind by a crashed process (or by OneDrive sync), turning "am I alone?"
// into stale-file heuristics. The kernel reaps mutexes the moment every
// owning handle is gone, so the answer is always live and crash-safe.
// ---------------------------------------------------------------------------

/// Returned by [`acquire_single_instance`] when this process may not claim
/// the guard: either another live process already owns it, or the mutex could
/// not be created at all.
///
/// Deliberately one opaque case. The two call sites print their own
/// operator-facing explanation (they know which name they tried), and any
/// *unexpected* `CreateMutexW` failure — malformed name, access denied —
/// collapses into the same answer on purpose: when exclusivity cannot be
/// proven, refusing to start is exactly as correct as "someone else is
/// running", because both prevent two daemons from silently fighting over
/// one pipe. Hard creation failures are in practice limited to bad names,
/// and ours are compile-time constants.
#[derive(Debug)]
pub struct AlreadyRunning;

/// An owning wrapper around a Win32 mutex handle that calls `CloseHandle` on
/// drop.
///
/// Why RAII matters doubly here: releasing the mutex is what *permits* the
/// next process to start, so ownership must be pinned to a lifetime the
/// programmer controls. Same pattern as [`OwnedProcessHandle`], but callers
/// MUST keep the returned value alive for the whole process lifetime
/// (binding it early in `main`) — dropping it early would silently re-enable
/// a second daemon mid-run.
#[derive(Debug)]
pub struct OwnedMutexHandle {
    handle: HANDLE,
}

impl OwnedMutexHandle {
    /// Borrows the raw handle for passing to other Win32 calls.
    pub fn as_raw(&self) -> HANDLE {
        self.handle
    }
}

impl Drop for OwnedMutexHandle {
    fn drop(&mut self) {
        // SAFETY: self.handle came from CreateMutexW (see
        // acquire_single_instance's contract), so closing it here exactly once
        // is correct. A CloseHandle failure has no recovery during drop; the
        // OS abandons (and releases) the mutex once all handles close anyway,
        // so ignoring the error cannot wedge future startups.
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

/// Acquires the named mutex as a single-instance guard, or reports
/// [`AlreadyRunning`].
///
/// `CreateMutexW` runs with `bInitialOwner = false`: the initial-owner flag
/// carries an ownership race (a creator is not reliably the owner if another
/// instance exits at the same instant), whereas the post-create
/// `ERROR_ALREADY_EXISTS` probe is atomic and unambiguous. Note that Win32
/// reports "already exists" through the thread's last error while still
/// returning a valid handle, so the probe must run before anything else
/// touches that state.
///
/// # Namespace guidance (callers choose; the name is taken verbatim)
///
/// Mutex names are case-insensitive and live in one of two kernel namespaces:
/// `Global\…` spans every login session on the machine — right for a
/// machine-wide service like the agent — while `Local\…` is private to the
/// caller's login session, right for a per-user helper where different
/// logged-in desktops must each run their own copy.
pub fn acquire_single_instance(name: &str) -> Result<OwnedMutexHandle, AlreadyRunning> {
    // NUL-terminated UTF-16 for the W-suffixed API; the buffer outlives the call.
    let mut wide: Vec<u16> = name.encode_utf16().collect();
    wide.push(0);

    // SAFETY: `wide` is a NUL-terminated UTF-16 buffer alive across the call,
    // passed read-only as PCWSTR, with default security attributes (None).
    // On the Ok path the windows crate performs no further Win32 calls, so
    // the GetLastError probe below still observes CreateMutexW's own status.
    let handle =
        unsafe { CreateMutexW(None, false, PCWSTR(wide.as_ptr())) }.map_err(|_| AlreadyRunning)?;

    // SAFETY: reading the thread's last error; no other Win32 call has
    // intervened since CreateMutexW above, so this still observes its status.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // Hand the duplicate handle straight back to the OS: the winner keeps
        // the mutex, we just leave quietly.
        drop(OwnedMutexHandle { handle });
        return Err(AlreadyRunning);
    }

    Ok(OwnedMutexHandle { handle })
}

/// Opens a process with `PROCESS_QUERY_LIMITED_INFORMATION` access.
///
/// Why limited rather than broad access: querying the image name needs only
/// this bit, which succeeds against elevated and protected processes without
/// debug privilege — the reason all three origin crates chose it.
pub fn open_process_query(pid: u32) -> windows::core::Result<OwnedProcessHandle> {
    // SAFETY: plain Win32 open of an arbitrary pid; failure surfaces as Err.
    // The returned handle is immediately wrapped in RAII so it is closed
    // exactly once on every path, including early returns by `?`.
    Ok(unsafe {
        OwnedProcessHandle::from_raw(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?)
    })
}

/// Full image path (e.g. `C:\...\chrome.exe`) of the process behind `handle`.
///
/// Uses `QueryFullProcessImageNameW` with the `PROCESS_NAME_WIN32` format,
/// which yields the drive-letter path users recognise, not an NT-device path.
pub fn image_path_from_handle(handle: &OwnedProcessHandle) -> windows::core::Result<String> {
    image_path_raw(handle.as_raw())
}

/// Full image path of the process `pid`, opening and closing its own handle.
///
/// Convenience form matching how tracker-win, enforce-win and session called
/// this today; prefer [`open_process_query`] plus [`image_path_from_handle`]
/// when several queries share one handle.
pub fn process_image_path(pid: u32) -> windows::core::Result<String> {
    let handle = open_process_query(pid)?;
    image_path_from_handle(&handle)
}

/// Single-shot `QueryFullProcessImageNameW` against a borrowed raw handle.
fn image_path_raw(handle: HANDLE) -> windows::core::Result<String> {
    // One fixed max-size allocation instead of growing: see
    // IMAGE_PATH_MAX_WCHARS for why 32k wide chars always suffices.
    let mut buf = vec![0u16; IMAGE_PATH_MAX_WCHARS];
    let mut len = buf.len() as u32;

    // SAFETY: buf outlives the call, len equals its exact length in u16s, and
    // PWSTR targets the buffer's start. The API writes at most *len units and
    // updates len to the copied count on success only.
    unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )?;
    }

    buf.truncate(len as usize);
    Ok(wide_to_string(&buf))
}

/// Decodes a NUL-terminated UTF-16 buffer into a `String`.
///
/// Stops at the first NUL (Win32 fixed-size buffers are usually zero-padded)
/// and replaces invalid UTF-16 lossily rather than failing, because a garbled
/// path must degrade to a wrong-but-stable app key, never to a hard error.
pub fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_to_string_stops_at_the_first_nul() {
        let mut buf = [0u16; 8];
        for (i, c) in "abc".encode_utf16().enumerate() {
            buf[i] = c;
        }
        assert_eq!(wide_to_string(&buf), "abc");
    }

    #[test]
    fn wide_to_string_without_a_nul_decodes_the_whole_buffer() {
        let chars: Vec<u16> = "abcdef".encode_utf16().collect();
        assert_eq!(wide_to_string(&chars), "abcdef");
    }

    #[test]
    fn wide_to_string_of_an_empty_slice_is_empty() {
        assert_eq!(wide_to_string(&[]), "");
    }

    #[test]
    fn wide_to_string_replaces_lone_surrogates_lossily() {
        assert_eq!(wide_to_string(&[0xD800, 0x0041]), "\u{FFFD}A");
    }

    #[test]
    fn open_process_query_pid_zero_is_an_error() {
        // Pid 0 (the Idle process) cannot be opened even with limited query
        // access, so this exercises the OpenProcess error path without
        // touching any real foreign process.
        assert!(open_process_query(0).is_err());
    }

    #[test]
    fn querying_a_bogus_handle_is_an_error() {
        // A null handle is never valid, so the FFI must return a clean Err
        // rather than garbage or a crash. Note INVALID_HANDLE_VALUE cannot be
        // used here: it doubles as GetCurrentProcess()'s pseudo-handle, so
        // queries against it succeed.
        let bogus = HANDLE(std::ptr::null_mut());
        assert!(image_path_raw(bogus).is_err());
    }

    #[test]
    fn image_path_of_our_own_process_succeeds() {
        // Our own pid is always openable and queryable, giving a positive-path
        // check that needs no foreign process.
        let path = process_image_path(std::process::id()).expect("self query must succeed");
        assert!(!path.is_empty());
        assert!(path.contains('\\'), "expected an absolute path, got {path}");
    }

    #[test]
    fn second_acquire_of_a_held_mutex_is_already_running() {
        // The pid suffix keeps concurrent test invocations on the same machine
        // from colliding; Local\ scope keeps them out of the global namespace.
        let name = format!(r"Local\st-win32-test-double-acquire-{}", std::process::id());
        let _guard = acquire_single_instance(&name).expect("first acquire of a fresh mutex");
        assert!(
            matches!(acquire_single_instance(&name), Err(AlreadyRunning)),
            "second in-process acquire of a held mutex must report AlreadyRunning"
        );
    }

    #[test]
    fn dropping_the_guard_handle_allows_reacquire() {
        let name = format!(r"Local\st-win32-test-reacquire-{}", std::process::id());
        {
            let _guard = acquire_single_instance(&name).expect("first acquire");
            // RAII release happens here; the kernel must honour it promptly.
        }
        let reacquired = acquire_single_instance(&name).expect("reacquire after Drop must succeed");
        drop(reacquired);
    }
}
