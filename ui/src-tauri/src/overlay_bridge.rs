//! Named pipe bridge server between `screentime-session` and the Tauri overlay window.
//!
//! Receives `Show` and `Hide` commands from `screentime-session` over
//! `\\.\pipe\screentime_overlay_bridge`, positions and sizes the secondary
//! `"overlay"` webview window over the target application, and exposes
//! commands to the React frontend for PIN verification and process termination.

use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{error_from, ipc_client, CmdResult, CommandError};
use st_ipc::{
    ErrorCode, LimitTargetDto, OverlayActiveStateDto, OverlayBridgeRequest, OverlayBridgeResponse,
    Response, OVERLAY_PIPE_NAME,
};

#[derive(Clone)]
pub struct OverlayBridgeState(pub Arc<Mutex<Option<OverlayActiveStateDto>>>);

impl OverlayBridgeState {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(None)))
    }
}

/// Program names the bridge accepts commands from.
const SESSION_EXE: [&str; 1] = ["screentime-session.exe"];

/// Overlay window floor sizes in logical pixels (scaled by the monitor's
/// scale factor in `place_overlay`). The block screen with its PIN pad open
/// needs about 800px of height and about 540px without it; below that the
/// page scrolls and drops its badge, so "Close" stays reachable. (At the old
/// 480 physical-pixel floor the actions were cut off.)
const MIN_WIDTH: u32 = 520;
const MIN_HEIGHT_PIN: u32 = 820;
const MIN_HEIGHT_BUTTONS: u32 = 560;

/// Lock the shared overlay state, recovering from poisoning: a panic in one
/// command must not take the block screen down with it.
fn lock_state(
    state: &Mutex<Option<OverlayActiveStateDto>>,
) -> std::sync::MutexGuard<'_, Option<OverlayActiveStateDto>> {
    state.lock().unwrap_or_else(|poisoned| {
        tracing::error!("overlay state mutex poisoned; recovering");
        poisoned.into_inner()
    })
}

/// Spawns the background listener thread for `\\.\pipe\screentime_overlay_bridge`.
///
/// `Show` is idempotent: the session helper re-sends it every couple of
/// seconds while a block holds, so a closed (Alt+F4) or hidden overlay, or a
/// restarted UI, comes back on its own. Only a changed target re-emits
/// `overlay_update`, so a PIN being typed is never reset.
pub fn start_bridge_server(
    app: AppHandle,
    bridge_state: Arc<Mutex<Option<OverlayActiveStateDto>>>,
) {
    std::thread::Builder::new()
        .name("screentime-overlay-bridge".into())
        .spawn(move || loop {
            match st_ipc::transport::server_accept(OVERLAY_PIPE_NAME) {
                Ok(mut stream) => {
                    // Only the session helper may drive the block screen. A
                    // definite mismatch is refused; an uninspectable peer is
                    // let through (and logged) rather than risk never showing
                    // a block.
                    #[cfg(windows)]
                    match st_win32::peer_is_trusted(stream.peer_process_id(), &SESSION_EXE) {
                        Some(false) => {
                            tracing::warn!(
                                "overlay bridge: refusing a client that is not the session helper"
                            );
                            let _ = st_ipc::write_message(
                                &mut stream,
                                &OverlayBridgeResponse::Error {
                                    message: "untrusted client".into(),
                                },
                            );
                            continue;
                        }
                        None => tracing::debug!("overlay bridge: client process not inspectable"),
                        Some(true) => {}
                    }
                    let req: Result<OverlayBridgeRequest, _> = st_ipc::read_message(&mut stream);
                    match req {
                        Ok(OverlayBridgeRequest::Show {
                            app_id,
                            label,
                            pin_locked,
                            target_hwnd,
                            process_id,
                            rect,
                        }) => {
                            let active = OverlayActiveStateDto {
                                app_id,
                                label,
                                pin_locked,
                                target_hwnd: target_hwnd.unwrap_or(0),
                                process_id,
                            };
                            let changed = {
                                let mut current = lock_state(&bridge_state);
                                let changed = current.as_ref() != Some(&active);
                                *current = Some(active.clone());
                                changed
                            };
                            if let Some(window) = app.get_webview_window("overlay") {
                                place_overlay(&window, rect, pin_locked);
                                let _ = window.set_always_on_top(true);
                                let _ = window.show();
                                let _ = window.set_focus();
                                if changed || !window.is_visible().unwrap_or(false) {
                                    let _ = app.emit("overlay_update", &active);
                                    let _ = window.emit("overlay_update", &active);
                                }
                            }
                            let _ = st_ipc::write_message(&mut stream, &OverlayBridgeResponse::Ack);
                        }
                        Ok(OverlayBridgeRequest::Hide) => {
                            hide_overlay_gracefully(&app, bridge_state.clone());
                            let _ = st_ipc::write_message(&mut stream, &OverlayBridgeResponse::Ack);
                        }
                        Ok(OverlayBridgeRequest::Ping) => {
                            let _ = st_ipc::write_message(&mut stream, &OverlayBridgeResponse::Ack);
                        }
                        Err(e) => {
                            tracing::debug!("overlay bridge read error: {e}");
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("overlay bridge accept error: {e}");
                    std::thread::sleep(std::time::Duration::from_millis(150));
                }
            }
        })
        .expect("spawning overlay bridge server thread");
}

/// Size the overlay to cover the target window (never smaller than the card
/// needs) and keep it inside the monitor the target is on.
fn place_overlay(
    window: &tauri::WebviewWindow,
    rect: Option<st_ipc::OverlayRectDto>,
    pin_locked: bool,
) {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    // The target's rect and the window size are physical pixels; the floors
    // are logical, so scale them for the monitor (1.5 at 150 %).
    let scale = monitor
        .as_ref()
        .map(|m| m.scale_factor())
        .or_else(|| window.scale_factor().ok())
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(1.0);
    let physical = |logical: u32| (f64::from(logical) * scale).round() as u32;
    let min_width = physical(MIN_WIDTH);
    let min_height = physical(if pin_locked {
        MIN_HEIGHT_PIN
    } else {
        MIN_HEIGHT_BUTTONS
    });
    let (mut width, mut height, center_x, center_y) = match (rect, &monitor) {
        (Some(r), _) => (
            (r.width.max(0) as u32).max(min_width),
            (r.height.max(0) as u32).max(min_height),
            r.x + r.width / 2,
            r.y + r.height / 2,
        ),
        (None, Some(mon)) => (
            physical(560),
            min_height,
            mon.position().x + mon.size().width as i32 / 2,
            mon.position().y + mon.size().height as i32 / 2,
        ),
        (None, None) => (physical(560), min_height, 400, 400),
    };
    let mut x = center_x - width as i32 / 2;
    let mut y = center_y - height as i32 / 2;
    if let Some(mon) = &monitor {
        let (mx, my) = (mon.position().x, mon.position().y);
        let (mw, mh) = (mon.size().width, mon.size().height);
        width = width.min(mw);
        height = height.min(mh);
        x = x.clamp(mx, mx + (mw - width) as i32);
        y = y.clamp(my, my + (mh - height) as i32);
    }
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width, height }));
}

/// Hide the overlay window after a short fade-out.
fn hide_overlay_gracefully(
    app: &AppHandle,
    bridge_state: Arc<Mutex<Option<OverlayActiveStateDto>>>,
) {
    *lock_state(&bridge_state) = None;
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = app.emit("overlay_graceful_exit", ());
        let _ = window.emit("overlay_graceful_exit", ());
        let _ = app.emit("overlay_hide", ());
        let _ = window.emit("overlay_hide", ());
        let win = window.clone();
        let state_clone = bridge_state.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(240));
            // A Show that arrived during the fade wins: only hide if nothing
            // re-armed the overlay meanwhile.
            if lock_state(&state_clone).is_none() {
                let _ = win.hide();
            }
        });
    }
}

#[tauri::command]
pub fn get_overlay_state(
    state: State<'_, OverlayBridgeState>,
) -> CmdResult<Option<OverlayActiveStateDto>> {
    Ok(lock_state(&state.0).clone())
}

#[tauri::command]
pub fn overlay_extend(
    app: AppHandle,
    state: State<'_, OverlayBridgeState>,
    app_id: i64,
    pin: String,
) -> CmdResult<bool> {
    let req = st_ipc::Request::GrantOverride {
        target: LimitTargetDto::App { id: app_id },
        seconds: 15 * 60,
        pin,
    };
    match ipc_client::request(req) {
        Ok(Response::Accepted { .. }) => {
            hide_overlay_gracefully(&app, state.0.clone());
            Ok(true)
        }
        Ok(Response::Error {
            code: ErrorCode::BadPin,
            message,
        }) => Err(error_from(ErrorCode::BadPin, message)),
        Ok(Response::Error { code, message }) => Err(error_from(code, message)),
        Ok(_) => Err(CommandError::unexpected()),
        Err(e) => Err(CommandError::unreachable(e)),
    }
}

/// "Quit" from the block screen. Tries a polite close and a direct terminate
/// from the user's session, then asks the agent (which runs elevated and can
/// close what the user cannot, e.g. an elevated game). The overlay hides only
/// when one of them actually worked; otherwise the error is returned and the
/// block screen stays up instead of uncovering the app.
#[tauri::command]
pub fn overlay_quit(
    app: AppHandle,
    state: State<'_, OverlayBridgeState>,
    app_id: i64,
    pid: u32,
    target_hwnd: i64,
) -> CmdResult<()> {
    #[allow(unused_mut)]
    let mut terminated_locally = false;
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
        use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

        if target_hwnd != 0 {
            let _ = PostMessageW(
                Some(HWND(target_hwnd as *mut std::ffi::c_void)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            );
        }
        if pid != 0 {
            if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                terminated_locally = TerminateProcess(handle, 1).is_ok();
                let _ = CloseHandle(handle);
            }
        }
    }
    #[cfg(not(windows))]
    let _ = (pid, target_hwnd);

    // Closing a blocked app needs no PIN (the agent's auth gate allows it).
    let agent = ipc_client::request(st_ipc::Request::CloseApps {
        app_id,
        pin: String::new(),
    });
    match agent {
        Ok(Response::Accepted { .. }) => {}
        _ if terminated_locally => {}
        Ok(Response::Error { code, message }) => return Err(error_from(code, message)),
        Ok(_) => return Err(CommandError::unexpected()),
        Err(e) => return Err(CommandError::unreachable(e)),
    }
    hide_overlay_gracefully(&app, state.0.clone());
    Ok(())
}

#[tauri::command]
pub fn hide_overlay_window(app: AppHandle, state: State<'_, OverlayBridgeState>) -> CmdResult<()> {
    hide_overlay_gracefully(&app, state.0.clone());
    Ok(())
}
