//! The tray panel: a small window (`tray` in tauri.conf.json, the React view
//! `?view=tray`) that opens from a left click on the tray icon and shows time
//! left today, the budgets and the next schedule. It sits next to the
//! taskbar, wherever the taskbar is, and hides when it loses focus.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WindowEvent};

/// Panel width in logical pixels (scaled by the monitor's factor).
const PANEL_W: u32 = 360;
/// Height bounds, logical pixels. The panel reports its content height
/// (`fit_tray_panel`) so it is only as tall as what it shows.
const PANEL_H_MIN: u32 = 160;
const PANEL_H_MAX: u32 = 560;
/// Height used when opening; the last fitted height, so reopening does not
/// jump.
static PANEL_H: AtomicU32 = AtomicU32::new(380);

/// Where the panel was last opened, so a new height keeps it against the
/// taskbar.
#[derive(Debug, Clone, Copy)]
struct Anchor {
    /// The click, physical pixels.
    click: (i32, i32),
    /// The monitor's work area, physical pixels.
    work: Area,
    scale: f64,
}

static LAST_ANCHOR: Mutex<Option<Anchor>> = Mutex::new(None);
/// Gap between the panel and the taskbar or screen edge, logical pixels.
const MARGIN: u32 = 12;

/// When the panel last hid itself on losing focus (ms since the epoch).
/// Clicking the tray icon while the panel is open first blurs it (hiding
/// it) and then delivers the click; without this the click would reopen it.
static LAST_BLUR_HIDE_MS: AtomicU64 = AtomicU64::new(0);
const REOPEN_GUARD_MS: u64 = 300;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A rectangle in physical pixels: x, y, width, height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Where the panel's top-left corner goes for a click at `cursor`, given the
/// monitor's work area (the screen minus the taskbar). The taskbar is on
/// the side the click is outside the work area; the panel opens from that
/// side, centered on the click along it, and always stays on screen.
pub fn panel_origin(cursor: (i32, i32), panel: (i32, i32), work: Area, margin: i32) -> (i32, i32) {
    let (cx, cy) = cursor;
    let (pw, ph) = panel;
    let clamp_x = |x: i32| {
        x.clamp(
            work.x + margin,
            (work.x + work.w - pw - margin).max(work.x + margin),
        )
    };
    let clamp_y = |y: i32| {
        y.clamp(
            work.y + margin,
            (work.y + work.h - ph - margin).max(work.y + margin),
        )
    };
    if cy >= work.y + work.h {
        // Taskbar at the bottom (the usual case).
        (clamp_x(cx - pw / 2), work.y + work.h - ph - margin)
    } else if cy < work.y {
        (clamp_x(cx - pw / 2), work.y + margin)
    } else if cx < work.x {
        (work.x + margin, clamp_y(cy - ph / 2))
    } else if cx >= work.x + work.w {
        (work.x + work.w - pw - margin, clamp_y(cy - ph / 2))
    } else {
        // Overflow menu or auto-hidden taskbar: open above the cursor.
        (clamp_x(cx - pw / 2), clamp_y(cy - ph - margin))
    }
}

/// Show the panel next to the tray at `cursor`, or hide it if it is open.
pub fn toggle(app: &AppHandle, cursor: PhysicalPosition<f64>) {
    let Some(panel) = app.get_webview_window("tray") else {
        return;
    };
    if panel.is_visible().unwrap_or(false) {
        let _ = panel.hide();
        return;
    }
    if now_ms().saturating_sub(LAST_BLUR_HIDE_MS.load(Ordering::Relaxed)) < REOPEN_GUARD_MS {
        return;
    }
    let monitor = app.monitor_from_point(cursor.x, cursor.y).ok().flatten();
    let scale = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
    let px = |logical: u32| (f64::from(logical) * scale).round() as i32;
    let (pw, ph) = (px(PANEL_W), px(PANEL_H.load(Ordering::Relaxed)));
    let work = monitor
        .as_ref()
        .map(|m| {
            let a = m.work_area();
            Area {
                x: a.position.x,
                y: a.position.y,
                w: a.size.width as i32,
                h: a.size.height as i32,
            }
        })
        .unwrap_or(Area {
            x: 0,
            y: 0,
            w: cursor.x as i32 + pw,
            h: cursor.y as i32,
        });
    let click = (cursor.x as i32, cursor.y as i32);
    if let Ok(mut last) = LAST_ANCHOR.lock() {
        *last = Some(Anchor { click, work, scale });
    }
    let (x, y) = panel_origin(click, (pw, ph), work, px(MARGIN));
    let _ = panel.set_size(tauri::Size::Physical(tauri::PhysicalSize {
        width: pw as u32,
        height: ph as u32,
    }));
    let _ = panel.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
    let _ = panel.show();
    let _ = panel.set_focus();
    // The panel reloads its numbers each time it opens.
    let _ = panel.emit("tray_panel_shown", ());
}

/// Resize the panel to its content (`height` in logical pixels, clamped),
/// keeping it against the taskbar where it was opened.
#[tauri::command]
pub fn fit_tray_panel(app: AppHandle, height: f64) {
    let Some(panel) = app.get_webview_window("tray") else {
        return;
    };
    if !height.is_finite() {
        return;
    }
    let logical = (height.round() as u32).clamp(PANEL_H_MIN, PANEL_H_MAX);
    PANEL_H.store(logical, Ordering::Relaxed);
    let anchor = LAST_ANCHOR.lock().ok().and_then(|a| *a);
    let Some(Anchor { click, work, scale }) = anchor else {
        return;
    };
    let px = |l: u32| (f64::from(l) * scale).round() as i32;
    let (pw, ph) = (px(PANEL_W), px(logical));
    let (x, y) = panel_origin(click, (pw, ph), work, px(MARGIN));
    let _ = panel.set_size(tauri::Size::Physical(tauri::PhysicalSize {
        width: pw as u32,
        height: ph as u32,
    }));
    let _ = panel.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
}

/// Hide the panel whenever it loses focus, like a Windows flyout.
pub fn setup(app: &tauri::App) {
    if let Some(panel) = app.get_webview_window("tray") {
        let handle = panel.clone();
        panel.on_window_event(move |event| {
            if let WindowEvent::Focused(false) = event {
                if handle.is_visible().unwrap_or(false) {
                    LAST_BLUR_HIDE_MS.store(now_ms(), Ordering::Relaxed);
                    let _ = handle.hide();
                }
            }
        });
    }
}

/// "Open Tether" in the panel: hide the panel and bring up the dashboard.
#[tauri::command]
pub fn open_dashboard(app: AppHandle) {
    if let Some(panel) = app.get_webview_window("tray") {
        let _ = panel.hide();
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::{panel_origin, Area};

    // A 1920x1080 screen with a 48px taskbar at the bottom.
    const WORK: Area = Area {
        x: 0,
        y: 0,
        w: 1920,
        h: 1032,
    };

    #[test]
    fn opens_above_a_bottom_taskbar_centered_on_the_click() {
        assert_eq!(
            panel_origin((1700, 1050), (360, 460), WORK, 12),
            (1520, 1032 - 460 - 12)
        );
    }

    #[test]
    fn never_runs_off_the_right_edge() {
        let (x, _) = panel_origin((1910, 1050), (360, 460), WORK, 12);
        assert_eq!(x, 1920 - 360 - 12);
    }

    #[test]
    fn opens_below_a_top_taskbar() {
        let work = Area {
            x: 0,
            y: 48,
            w: 1920,
            h: 1032,
        };
        assert_eq!(panel_origin((1700, 20), (360, 460), work, 12), (1520, 60));
    }

    #[test]
    fn opens_beside_side_taskbars() {
        let left = Area {
            x: 60,
            y: 0,
            w: 1860,
            h: 1080,
        };
        assert_eq!(
            panel_origin((30, 900), (360, 460), left, 12),
            (72, 1080 - 460 - 12)
        );
        let right = Area {
            x: 0,
            y: 0,
            w: 1860,
            h: 1080,
        };
        assert_eq!(
            panel_origin((1890, 500), (360, 460), right, 12),
            (1860 - 360 - 12, 270)
        );
    }

    #[test]
    fn a_click_inside_the_work_area_opens_above_the_cursor() {
        assert_eq!(panel_origin((900, 800), (360, 460), WORK, 12), (720, 328));
    }
}
