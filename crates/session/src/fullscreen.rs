//! Should the HUD stay out of the way? Fullscreen, game and overlay-focus detection.

use super::*;

#[cfg(windows)]
/// Checks whether the foreground window is specifically the secondary "Tether Overlay"
/// window, as opposed to the main Tether dashboard window or any other application.
pub(crate) fn is_overlay_window_focused() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return false;
        }
        let mut buf = [0u16; 256];
        let len = GetWindowTextW(hwnd, &mut buf);
        if len <= 0 {
            return false;
        }
        let title = String::from_utf16_lossy(&buf[..len as usize]);
        title.contains("Tether Overlay")
    }
}

#[cfg(not(windows))]
pub(crate) fn is_overlay_window_focused() -> bool {
    false
}

#[cfg(windows)]
/// Checks whether a window is running a 3D game or full-screen display,
/// where rendering a top-level Win32 HUD overlay would force DWM Composed Flip,
/// overriding in-game frame caps, VRR (FreeSync/G-Sync), and driver limiters (AMD Chill/FRTC).
/// Determines if a window's bounds and window style correspond to a true borderless fullscreen
/// window (such as a game, full-screen video, or F11 display) rather than a standard maximized
/// desktop application.
///
/// Windows maximized normally respect the monitor's work area (`rcWork`) and leave the taskbar
/// visible. True fullscreen applications cover the physical monitor (`rcMonitor`), obscuring
/// the taskbar, and lack standard window captions (`WS_CAPTION`).
#[cfg(windows)]
pub(crate) fn is_window_rect_fullscreen(
    wr: windows::Win32::Foundation::RECT,
    rc_monitor: windows::Win32::Foundation::RECT,
    rc_work: windows::Win32::Foundation::RECT,
    style: u32,
) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{WS_CAPTION, WS_MAXIMIZE};

    // 1. Taskbar occlusion check:
    // If a taskbar (or docked appbar) is present on this monitor, a standard maximized window
    // stops at the boundary of rcWork (with up to an 8px invisible resize margin).
    // If the window leaves the taskbar uncovered by more than 12px, it is not fullscreen.
    let leaves_taskbar_uncovered = (rc_work.bottom < rc_monitor.bottom
        && wr.bottom < rc_monitor.bottom - 12)
        || (rc_work.top > rc_monitor.top && wr.top > rc_monitor.top + 12)
        || (rc_work.left > rc_monitor.left && wr.left > rc_monitor.left + 12)
        || (rc_work.right < rc_monitor.right && wr.right < rc_monitor.right - 12);

    if leaves_taskbar_uncovered {
        return false;
    }

    // 2. Physical monitor coverage check:
    // Fullscreen windows cover rcMonitor completely, allowing a tight margin (10px) for
    // high-DPI scaling, invisible borders, or rounding.
    let covers_monitor = wr.left <= rc_monitor.left + 10
        && wr.top <= rc_monitor.top + 10
        && wr.right >= rc_monitor.right - 10
        && wr.bottom >= rc_monitor.bottom - 10;

    if !covers_monitor {
        return false;
    }

    // 3. Window style check:
    // Standard desktop applications maximized with window chrome/decorations have both
    // WS_MAXIMIZE and WS_CAPTION. Even with auto-hidden taskbars, these remain desktop apps.
    // Fullscreen games and true fullscreen media/F11 windows either lack WS_CAPTION or are
    // unmaximized windows explicitly sized to cover the entire monitor.
    let is_maximized = (style & WS_MAXIMIZE.0) != 0;
    let has_caption = (style & WS_CAPTION.0) == WS_CAPTION.0;
    if is_maximized && has_caption {
        return false;
    }

    true
}

#[cfg(windows)]
/// Checks whether a window is running a 3D game or full-screen display,
/// where rendering a top-level Win32 HUD overlay would force DWM Composed Flip,
/// overriding in-game frame caps, VRR (FreeSync/G-Sync), and driver limiters (AMD Chill/FRTC).
pub(crate) fn is_game_or_fullscreen(snap: &snapshot::FocusedSnapshot) -> bool {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetWindowLongW, GWL_STYLE};

    // 1. Path-based detection (known game launcher directories)
    if let st_core::model::AppKey::WindowsExe(ref path) = snap.key {
        if st_core::games::is_game_path(path) {
            return true;
        }
    }

    // 2. Executable name pattern detection
    if st_core::games::is_game_executable(snap.key.basename()) {
        return true;
    }

    let hwnd = HWND(snap.hwnd as *mut std::ffi::c_void);
    if hwnd.0.is_null() {
        return false;
    }

    unsafe {
        // 3. Game engine window class detection (Unreal, Unity, Source, SDL, GLFW, Godot)
        let mut class_buf = [0u16; 256];
        let class_len = GetClassNameW(hwnd, &mut class_buf);
        if class_len > 0 {
            let class_name =
                String::from_utf16_lossy(&class_buf[..class_len as usize]).to_ascii_lowercase();
            const GAME_CLASSES: &[&str] = &[
                "unrealwindow",
                "unitywndclass",
                "valve001",
                "glfw30",
                "sdl_app",
                "godot_engine",
            ];
            if GAME_CLASSES.iter().any(|c| class_name.contains(c))
                || class_name.contains("direct3d")
                || class_name.contains("renderwindow")
            {
                return true;
            }
        }

        // 4. Direct D3D Fullscreen / Presentation check via Windows Shell notification state
        if let Ok(state) = SHQueryUserNotificationState() {
            if state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE {
                return true;
            }
        }

        // 5. Geometry check: does the window truly cover the physical monitor (borderless fullscreen)?
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            let (x, y, w, h) = snap.rect;
            let wr = RECT {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            };
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            if is_window_rect_fullscreen(wr, mi.rcMonitor, mi.rcWork, style) {
                return true;
            }
        }
    }

    false
}

#[cfg(not(windows))]
pub(crate) fn is_game_or_fullscreen(_snap: &snapshot::FocusedSnapshot) -> bool {
    false
}
