//! Freezing, thawing and (reluctantly) terminating Windows processes.
//!
//! # Why freeze instead of kill
//!
//! Killing an app to enforce a time limit destroys unsaved work, and a tool
//! that loses your document once will be uninstalled the same day. Suspending
//! every thread instead leaves the process intact: it can be resumed at the day
//! boundary or when an override is granted, with the user's state untouched.
//!
//! `NtSuspendProcess` is undocumented but has been present and stable since
//! Windows XP. It is resolved dynamically so that its absence degrades to
//! "cannot freeze" rather than preventing the agent from starting at all.
//!
//! # Verification status
//!
//! M0 spike target. Validate on Windows 10 and 11 against: a plain Win32 app, a
//! browser (multi-process), an Electron app, and an elevated process.

use std::mem::size_of;

use st_core::model::AppKey;
use st_core::platform::{PlatformError, PlatformResult, ProcessController};
use st_win32::{image_path_from_handle, open_process_query, wide_to_string};

use windows::core::{s, PCSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows::Win32::System::Threading::{
    OpenProcess, TerminateProcess, PROCESS_SUSPEND_RESUME, PROCESS_TERMINATE,
};

/// `NTSTATUS`-returning single-argument ntdll function.
type NtProcessFn = unsafe extern "system" fn(HANDLE) -> i32;

#[derive(Default)]
pub struct Win32ProcessController {
    suspend: Option<NtProcessFn>,
    resume: Option<NtProcessFn>,
}

impl Win32ProcessController {
    pub fn new() -> Self {
        // SAFETY: ntdll is loaded into every Win32 process, so GetModuleHandleA
        // cannot race a load/unload here. Both symbols have the documented
        // `NTSTATUS(HANDLE)` shape.
        unsafe {
            let suspend = resolve_ntdll(s!("NtSuspendProcess"));
            let resume = resolve_ntdll(s!("NtResumeProcess"));
            if suspend.is_none() || resume.is_none() {
                tracing::warn!(
                    "ntdll suspend/resume unavailable; blocking will fall back to terminate"
                );
            }
            Self { suspend, resume }
        }
    }

    fn call(&self, f: Option<NtProcessFn>, pid: u32, access_terminate: bool) -> PlatformResult<()> {
        let Some(f) = f else {
            return Err(PlatformError::Unsupported(
                "NtSuspendProcess/NtResumeProcess",
            ));
        };
        let access = if access_terminate {
            PROCESS_TERMINATE
        } else {
            PROCESS_SUSPEND_RESUME
        };

        // SAFETY: handle is opened here and closed on every path.
        unsafe {
            let handle =
                OpenProcess(access, false, pid).map_err(|_| PlatformError::ProcessGone(pid))?;
            let status = f(handle);
            let _ = CloseHandle(handle);

            if status < 0 {
                return Err(PlatformError::Other(format!(
                    "ntdll call for pid {pid} returned NTSTATUS 0x{status:08x}"
                )));
            }
        }
        Ok(())
    }
}

impl ProcessController for Win32ProcessController {
    /// Every live PID whose image path matches `key`.
    ///
    /// Returns the whole set rather than one PID because browsers and Electron
    /// apps are process trees; suspending only the parent leaves the renderers
    /// running and the window responsive.
    fn find_processes(&mut self, key: &AppKey) -> PlatformResult<Vec<u32>> {
        let AppKey::WindowsExe(target_path) = key else {
            // Packaged apps need AUMID-based lookup, which is M1 work.
            return Err(PlatformError::Unsupported(
                "process lookup for non-exe app keys",
            ));
        };
        let target_basename = key.basename().to_string();

        // SAFETY: the snapshot handle is closed on all paths, and PROCESSENTRY32W
        // has its dwSize set before the first call as the API requires.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
                .map_err(|e| PlatformError::Other(format!("CreateToolhelp32Snapshot: {e}")))?;
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(PlatformError::Other("invalid process snapshot".into()));
            }

            let mut entry = PROCESSENTRY32W {
                dwSize: size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };

            let mut pids = Vec::new();
            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    let name = wide_to_string(&entry.szExeFile).to_lowercase();
                    // Cheap name filter first; the full-path check needs a
                    // handle per process, which is far more expensive.
                    if name == target_basename {
                        // Image-path querying is delegated to st_win32, the
                        // workspace's canonical Win32 plumbing: this crate used
                        // to carry a private OpenProcess +
                        // QueryFullProcessImageNameW + UTF-16-decode copy that
                        // would drift exactly as that crate's docs warn.
                        if let Ok(handle) = open_process_query(entry.th32ProcessID) {
                            if let Ok(path) = image_path_from_handle(&handle) {
                                if path.replace('/', "\\").to_lowercase() == *target_path {
                                    pids.push(entry.th32ProcessID);
                                }
                            }
                        }
                    }
                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }

            let _ = CloseHandle(snapshot);
            Ok(pids)
        }
    }

    fn freeze(&mut self, pid: u32) -> PlatformResult<()> {
        self.call(self.suspend, pid, false)
    }

    fn thaw(&mut self, pid: u32) -> PlatformResult<()> {
        self.call(self.resume, pid, false)
    }

    /// Last resort, and only after the grace countdown has elapsed.
    fn terminate(&mut self, pid: u32) -> PlatformResult<()> {
        // SAFETY: handle opened and closed locally.
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, false, pid)
                .map_err(|_| PlatformError::ProcessGone(pid))?;
            let result = TerminateProcess(handle, 1);
            let _ = CloseHandle(handle);
            result.map_err(|e| PlatformError::Other(format!("TerminateProcess({pid}): {e}")))
        }
    }

    fn backend(&self) -> &'static str {
        "win32-ntsuspend"
    }
}

unsafe fn resolve_ntdll(name: PCSTR) -> Option<NtProcessFn> {
    let ntdll = GetModuleHandleA(s!("ntdll.dll")).ok()?;
    let addr = GetProcAddress(ntdll, name)?;
    Some(std::mem::transmute::<
        unsafe extern "system" fn() -> isize,
        NtProcessFn,
    >(addr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_to_string_stops_at_the_nul() {
        // Now exercised through the adopted st_win32 re-export; the private
        // copy this crate used to own is gone.
        let mut buf = [0u16; 8];
        for (i, c) in "abc".encode_utf16().enumerate() {
            buf[i] = c;
        }
        assert_eq!(wide_to_string(&buf), "abc");
    }

    #[test]
    fn packaged_apps_are_reported_as_unsupported_not_silently_ignored() {
        let mut c = Win32ProcessController::new();
        let key = AppKey::WindowsAumid("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".into());
        assert!(matches!(
            c.find_processes(&key),
            Err(PlatformError::Unsupported(_))
        ));
    }
}
