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

/// Spawns the background listener thread for `\\.\pipe\screentime_overlay_bridge`.
pub fn start_bridge_server(
    app: AppHandle,
    bridge_state: Arc<Mutex<Option<OverlayActiveStateDto>>>,
) {
    std::thread::Builder::new()
        .name("screentime-overlay-bridge".into())
        .spawn(move || loop {
            match st_ipc::transport::server_accept(OVERLAY_PIPE_NAME) {
                Ok(mut stream) => {
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
                            *bridge_state.lock().unwrap() = Some(active.clone());

                            if let Some(window) = app.get_webview_window("overlay") {
                                if let Some(r) = rect {
                                    // Sized to cover the target application window exactly,
                                    // ensuring comfortable space for the 480px card while
                                    // NEVER expanding over the Windows taskbar.
                                    let width = (r.width as u32).max(520);
                                    let height = (r.height as u32).max(480);
                                    let center_x = r.x + r.width / 2;
                                    let center_y = r.y + r.height / 2;
                                    let x = center_x - (width as i32) / 2;
                                    let y = center_y - (height as i32) / 2;

                                    let _ = window.set_position(tauri::Position::Physical(
                                        tauri::PhysicalPosition { x, y },
                                    ));
                                    let _ = window.set_size(tauri::Size::Physical(
                                        tauri::PhysicalSize { width, height },
                                    ));
                                } else if let Ok(Some(mon)) = window.primary_monitor() {
                                    let pos = mon.position();
                                    let size = mon.size();
                                    let width = 560u32;
                                    let height = 520u32;
                                    let x = pos.x + (size.width as i32 - width as i32) / 2;
                                    let y = pos.y + (size.height as i32 - height as i32) / 2;
                                    let _ = window.set_position(tauri::Position::Physical(
                                        tauri::PhysicalPosition { x, y },
                                    ));
                                    let _ = window.set_size(tauri::Size::Physical(
                                        tauri::PhysicalSize { width, height },
                                    ));
                                }

                                let _ = window.set_always_on_top(true);
                                let _ = window.show();
                                let _ = window.set_focus();
                                let _ = app.emit("overlay_update", &active);
                                let _ = window.emit("overlay_update", &active);
                            }
                            let _ = st_ipc::write_message(&mut stream, &OverlayBridgeResponse::Ack);
                        }
                        Ok(OverlayBridgeRequest::Hide) => {
                            *bridge_state.lock().unwrap() = None;
                            if let Some(window) = app.get_webview_window("overlay") {
                                let _ = window.hide();
                                let _ = app.emit("overlay_hide", ());
                                let _ = window.emit("overlay_hide", ());
                            }
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

/// Gracefully hide the overlay window with a subtle fade-out transition.
fn hide_overlay_gracefully(
    app: &AppHandle,
    bridge_state: Arc<Mutex<Option<OverlayActiveStateDto>>>,
) {
    *bridge_state.lock().unwrap() = None;
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = app.emit("overlay_hide", ());
        let _ = window.emit("overlay_hide", ());
        let win = window.clone();
        let state_clone = bridge_state.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(240));
            if state_clone.lock().unwrap().is_none() {
                let _ = win.hide();
            }
        });
    }
}

#[tauri::command]
pub fn get_overlay_state(
    state: State<'_, OverlayBridgeState>,
) -> CmdResult<Option<OverlayActiveStateDto>> {
    Ok(state.0.lock().unwrap().clone())
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

#[tauri::command]
pub fn overlay_quit(
    app: AppHandle,
    state: State<'_, OverlayBridgeState>,
    app_id: i64,
    pid: u32,
    target_hwnd: i64,
) -> CmdResult<()> {
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
                let _ = TerminateProcess(handle, 1);
                let _ = CloseHandle(handle);
            }
        }
    }

    let _ = ipc_client::request(st_ipc::Request::CloseApps {
        app_id,
        pin: String::new(),
    });

    hide_overlay_gracefully(&app, state.0.clone());
    Ok(())
}

#[tauri::command]
pub fn hide_overlay_window(app: AppHandle, state: State<'_, OverlayBridgeState>) -> CmdResult<()> {
    hide_overlay_gracefully(&app, state.0.clone());
    Ok(())
}
