//! Finding and closing a blocked app's processes on Windows.
//!
//! Blocking itself is not done here: the session helper covers a blocked app
//! with the block screen (an always-on-top window), so the app keeps running
//! and nothing is lost. This controller is used only when the person chooses
//! Close on the block screen, as the agent-side fallback for `CloseApps`.

use std::mem::size_of;

use st_core::model::AppKey;
use st_core::platform::{PlatformError, PlatformResult, ProcessController};
use st_win32::{image_path_from_handle, open_process_query, wide_to_string};

use windows::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

#[derive(Default)]
pub struct Win32ProcessController;

impl Win32ProcessController {
    pub fn new() -> Self {
        Self
    }
}

impl ProcessController for Win32ProcessController {
    /// Every live PID whose image path matches `key`.
    ///
    /// Returns the whole set rather than one PID because browsers and Electron
    /// apps are process trees; closing only the parent leaves the renderers
    /// running and the window responsive.
    fn find_processes(&mut self, key: &AppKey) -> PlatformResult<Vec<u32>> {
        let AppKey::WindowsExe(target_path) = key else {
            // Packaged apps need AUMID-based lookup, not implemented yet.
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
                                let norm = path.replace('/', "\\").to_lowercase();
                                if norm == *target_path
                                    || norm.ends_with(&format!("\\{target_basename}"))
                                {
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

    /// Ends one process. Called only for an app the person chose to close.
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
        "win32-overlay"
    }
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
