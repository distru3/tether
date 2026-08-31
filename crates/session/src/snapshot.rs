//! One-shot foreground snapshot: subject key and screen rect from a single
//! `GetForegroundWindow` handle.
//!
//! # Why one handle must drive both reads
//!
//! Audit fix: the old code called `GetForegroundWindow` twice — once for the
//! app key, once for the overlay rect. Between those calls the user can
//! Alt-Tab, so the overlay could decide "app X is blocked" and then position
//! itself over app Y's window (or vice versa). Taking every read from one
//! `HWND` makes subject and geometry atomic; if focus changed underneath us,
//! both readings at least describe the *same* instant.

use st_core::model::AppKey;

/// What had focus and exactly where its window sits.
///
/// `rect` is `(x, y, width, height)`; `(0, 0, 0, 0)` means "no usable
/// geometry" and callers treat that as "do not show an overlay this cycle"
/// (the same convention the previous code used).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusedSnapshot {
    pub key: AppKey,
    pub rect: (i32, i32, i32, i32),
    pub pid: u32,
    pub hwnd: isize,
}

#[cfg(windows)]
pub fn focused_snapshot() -> Option<FocusedSnapshot> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
    };

    // SAFETY: every call below is a plain Win32 query on handles we do not own:
    // GetForegroundWindow borrows the system's window, and the process handle
    // opened for the image-path query is created and closed inside
    // st_win32::process_image_path.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            // Locked session, desktop, or compositor restart: normal.
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }

        // Canonical image path via the shared implementation; elevated and
        // protected processes included (PROCESS_QUERY_LIMITED_INFORMATION).
        let path = st_win32::process_image_path(pid).ok()?;
        let key = AppKey::windows_exe(&path);

        let mut r = RECT::default();
        let rect = if GetWindowRect(hwnd, &mut r).is_ok() {
            let w = r.right - r.left;
            let h = r.bottom - r.top;
            if w > 0 && h > 0 {
                (r.left, r.top, w, h)
            } else {
                (0, 0, 0, 0)
            }
        } else {
            (0, 0, 0, 0)
        };
        Some(FocusedSnapshot {
            key,
            rect,
            pid,
            hwnd: hwnd.0 as isize,
        })
    }
}

#[cfg(not(windows))]
pub fn focused_snapshot() -> Option<FocusedSnapshot> {
    None
}
