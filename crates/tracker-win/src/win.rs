//! Win32 foreground-window and idle detection.
//!
//! # Verification status
//!
//! This is the M0 spike target. The API choices below are deliberate, but the
//! exact `windows` crate signatures shift between minor versions, so treat the
//! first `cargo build` as part of the spike rather than a formality.
//!
//! # Known gaps, tracked for M0/M1
//!
//! * **Packaged (UWP/Store) apps.** Their foreground window belongs to
//!   `ApplicationFrameHost.exe`, so every Store app currently reports as that
//!   single host process. Fixing it means enumerating child windows to find the
//!   one whose owning process differs from the host, then reading its
//!   Application User Model ID. Until then, Store apps are lumped together.
//! * **Session lock.** Detected only indirectly here (the foreground window
//!   becomes `LogonUI.exe` or nothing). The real fix is subscribing to
//!   `WTSRegisterSessionNotification` in the session helper and pushing an
//!   explicit lock event.
//! * **Media playback.** A user watching a two-hour film generates no input and
//!   will be scored as idle. Needs an audio-session check before shipping.

use std::mem::size_of;

use st_core::model::AppKey;
use st_core::platform::{
    ActiveWindow, IdleMonitor, IdleState, PlatformError, PlatformResult, WindowTracker,
};

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
};

/// Processes that are never the "app the user is using", even when they own the
/// foreground window.
const SHELL_PROCESSES: &[&str] = &[
    // The desktop and taskbar. Focus lands here between apps and while the
    // Start menu is open; crediting it would invent hours of phantom usage.
    "explorer.exe",
    // Lock screen and credential UI.
    "logonui.exe",
    "lockapp.exe",
    // Alt-Tab / task switcher and other shell surfaces.
    "searchhost.exe",
    "startmenuexperiencehost.exe",
    "shellexperiencehost.exe",
    "textinputhost.exe",
];

pub struct Win32WindowTracker {
    capture_titles: bool,
}

impl Win32WindowTracker {
    /// `capture_titles` should mirror the `capture_window_titles` setting.
    /// Window titles leak document names, page titles and occasionally
    /// credentials, so this defaults to off in the database.
    pub fn new(capture_titles: bool) -> Self {
        Self { capture_titles }
    }
}

impl WindowTracker for Win32WindowTracker {
    fn active_window(&mut self) -> PlatformResult<Option<ActiveWindow>> {
        // SAFETY: every call below is a plain Win32 query. The handle from
        // OpenProcess is closed on all paths, and the wide-string buffer is
        // sized from MAX_PATH-style bounds before being read back.
        unsafe {
            let hwnd: HWND = GetForegroundWindow();
            if hwnd.0.is_null() {
                // No focused window: locked, on the desktop, or the compositor
                // is mid-restart. Normal, not an error.
                return Ok(None);
            }

            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return Ok(None);
            }

            let path = process_image_path(pid)?;
            let key = AppKey::windows_exe(&path);

            if SHELL_PROCESSES.contains(&key.basename()) {
                return Ok(None);
            }

            let title = if self.capture_titles {
                window_title(hwnd)
            } else {
                None
            };

            Ok(Some(ActiveWindow {
                display_name: friendly_name(&path),
                key,
                pid,
                title,
            }))
        }
    }

    fn backend(&self) -> &'static str {
        "win32"
    }
}

/// Full image path of a process, via `QueryFullProcessImageNameW`.
///
/// `PROCESS_QUERY_LIMITED_INFORMATION` is used rather than the broader
/// `PROCESS_QUERY_INFORMATION` because it succeeds against elevated and
/// protected processes without needing debug privilege.
unsafe fn process_image_path(pid: u32) -> PlatformResult<String> {
    let handle: HANDLE = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
        .map_err(|_| PlatformError::ProcessGone(pid))?;

    // Long paths can exceed MAX_PATH; 32k wide chars is the documented ceiling.
    let mut buf = vec![0u16; 32_768];
    let mut len = buf.len() as u32;
    let result = QueryFullProcessImageNameW(
        handle,
        PROCESS_NAME_WIN32,
        PWSTR(buf.as_mut_ptr()),
        &mut len,
    );
    let _ = CloseHandle(handle);

    result.map_err(|e| {
        PlatformError::Other(format!(
            "QueryFullProcessImageNameW failed for pid {pid}: {e}"
        ))
    })?;

    buf.truncate(len as usize);
    Ok(String::from_utf16_lossy(&buf))
}

unsafe fn window_title(hwnd: HWND) -> Option<String> {
    let mut buf = [0u16; 512];
    let len = GetWindowTextW(hwnd, &mut buf);
    if len <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

/// Placeholder display name derived from the file name.
///
/// M1 replaces this with the `FileDescription` field from the executable's
/// version resource, which is what Task Manager shows ("Google Chrome" rather
/// than "chrome.exe").
fn friendly_name(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let stem = file.strip_suffix(".exe").unwrap_or(file);
    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => stem.to_string(),
    }
}

pub struct Win32IdleMonitor {
    threshold_secs: u64,
}

impl Win32IdleMonitor {
    pub fn new(threshold_secs: u64) -> Self {
        Self { threshold_secs }
    }
}

impl IdleMonitor for Win32IdleMonitor {
    fn idle_state(&mut self) -> PlatformResult<IdleState> {
        // SAFETY: GetLastInputInfo writes into a correctly sized struct;
        // GetTickCount has no preconditions.
        unsafe {
            let mut info = LASTINPUTINFO {
                cbSize: size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };
            if !GetLastInputInfo(&mut info).as_bool() {
                return Err(PlatformError::Other("GetLastInputInfo failed".into()));
            }

            // GetTickCount wraps every ~49.7 days, so subtract with wrapping.
            let idle_ms = GetTickCount().wrapping_sub(info.dwTime);
            let idle_secs = u64::from(idle_ms) / 1000;

            Ok(if idle_secs >= self.threshold_secs {
                IdleState::Idle {
                    for_secs: idle_secs,
                }
            } else {
                IdleState::Active
            })
        }
    }

    fn backend(&self) -> &'static str {
        "win32-lastinput"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_name_strips_path_and_extension() {
        assert_eq!(friendly_name("C:\\Program Files\\Foo\\bar.exe"), "Bar");
        assert_eq!(friendly_name("chrome.exe"), "Chrome");
    }

    #[test]
    fn shell_processes_are_matched_case_insensitively() {
        // AppKey::windows_exe lower-cases, so the list only needs lower-case
        // entries. This test locks that invariant in.
        let key = AppKey::windows_exe("C:\\Windows\\Explorer.EXE");
        assert!(SHELL_PROCESSES.contains(&key.basename()));
    }

    #[test]
    fn shell_process_list_is_lowercase() {
        for p in SHELL_PROCESSES {
            assert_eq!(*p, p.to_lowercase(), "{p} must be lower-case to match");
        }
    }
}
