//! `screentime-session` — the per-login-session helper.
//!
//! # Why this binary exists
//!
//! On Windows, a service running under `LocalSystem` lives in Session 0 and
//! cannot see the interactive desktop, so `GetForegroundWindow` returns nothing
//! useful and it cannot display UI. Anything that needs the user's screen must
//! live in a process that starts *inside* the login session.
//!
//! M2.5 responsibility: draw the block overlay. The agent records which apps
//! are blocked; this helper polls for them, and when the focused app is one of
//! them it shows a topmost, input-blocking overlay over it (see [`overlay`]).

mod overlay;

use std::time::Duration;

use st_core::model::AppKey;
use st_ipc::{transport, Request, Response};

/// The agent's pipe, same name the UI and agent use.
const PIPE_NAME: &str = "screentime";

/// How often to re-check the agent for blocked apps and refocus.
const POLL: Duration = Duration::from_millis(1000);

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("screentime-session running");

    // The overlay blocks in a message loop, so the poll loop and the overlay
    // each get a thread. `overlay_active` tracks any current overlay.
    let mut overlay_active: Option<ActiveOverlay> = None;

    loop {
        match transport::client_connect(PIPE_NAME) {
            Err(e) => {
                tracing::warn!(error = %e, "agent not reachable; retrying");
                std::thread::sleep(POLL);
                continue;
            }
            Ok(mut stream) => {
                let blocked = match st_ipc::write_message(&mut stream, &Request::BlockedApps) {
                    Ok(()) => read_blocked(&mut stream),
                    Err(e) => {
                        tracing::warn!(error = %e, "blocked-apps request failed");
                        None
                    }
                };
                if let Some(blocked) = blocked {
                    drive_overlay(&mut overlay_active, &blocked);
                }
            }
        }
        std::thread::sleep(POLL);
    }
}

/// Read the `BlockedApps` response; returns `None` on any error.
fn read_blocked(
    stream: &mut (impl std::io::Read + std::io::Write),
) -> Option<Vec<st_ipc::BlockedAppDto>> {
    match st_ipc::read_message::<_, Response>(stream) {
        Ok(Response::BlockedApps(apps)) => Some(apps),
        Ok(other) => {
            tracing::warn!(?other, "unexpected response to BlockedApps");
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "blocked-apps response failed");
            None
        }
    }
}

/// An active overlay: its thread and the control used to dismiss it.
type ActiveOverlay = (
    std::thread::JoinHandle<()>,
    std::sync::Arc<overlay::OverlayControl>,
);

/// If the focused app is blocked, ensure an overlay is showing; otherwise
/// ensure it is gone.
fn drive_overlay(overlay_active: &mut Option<ActiveOverlay>, blocked: &[st_ipc::BlockedAppDto]) {
    let focused = focused_window_key();
    let focused_blocked = focused.as_ref().and_then(|key| {
        blocked
            .iter()
            .find(|b| app_key_matches(&b.app_key, key))
            .map(|b| b.app_id)
    });

    match (focused_blocked, overlay_active.is_some()) {
        (Some(app_id), false) => {
            let blocked_app = blocked.iter().find(|b| b.app_id == app_id).cloned();
            tracing::info!(app = ?app_id, "showing block overlay");
            let rect = focused_window_rect();
            let callbacks = overlay::OverlayCallbacks::new(
                move |pin| close_app(app_id, pin),
                move |pin| extend_app(app_id, pin),
            );
            let control = overlay::OverlayControl::new();
            let control2 = control.clone();
            let handle = std::thread::spawn(move || {
                overlay::run_overlay(rect, callbacks, control2);
            });
            *overlay_active = Some((handle, control));
            let _ = blocked_app;
        }
        (None, true) => {
            tracing::info!("block lifted; dismissing overlay");
            if let Some((_handle, control)) = overlay_active.take() {
                control.dismiss();
            }
        }
        _ => {}
    }
}

/// "Quit" from the overlay: terminate the app's process tree (PIN-gated).
fn close_app(app_id: i64, pin: &str) -> bool {
    match request_agent(Request::CloseApps {
        app_id,
        pin: pin.to_string(),
    }) {
        Ok(Response::Accepted { .. }) => true,
        Ok(Response::Error {
            code: st_ipc::ErrorCode::BadPin,
            ..
        }) => {
            tracing::warn!("wrong PIN on quit");
            false
        }
        Ok(other) => {
            tracing::warn!(?other, "unexpected close response");
            false
        }
        Err(e) => {
            tracing::warn!(error = %e, "close request failed");
            false
        }
    }
}

/// "+15 minutes" from the overlay (PIN-gated).
fn extend_app(app_id: i64, pin: &str) -> bool {
    match request_agent(Request::GrantOverride {
        target: st_ipc::LimitTargetDto::App { id: app_id },
        seconds: 15 * 60,
        pin: pin.to_string(),
    }) {
        Ok(Response::Accepted { .. }) => true,
        Ok(Response::Error {
            code: st_ipc::ErrorCode::BadPin,
            ..
        }) => {
            tracing::warn!("wrong PIN on extend");
            false
        }
        Ok(other) => {
            tracing::warn!(?other, "unexpected extend response");
            false
        }
        Err(e) => {
            tracing::warn!(error = %e, "extend request failed");
            false
        }
    }
}

/// One-shot IPC round trip to the agent, used by the overlay callbacks.
fn request_agent(request: Request) -> anyhow::Result<Response> {
    let mut stream = transport::client_connect(PIPE_NAME)?;
    st_ipc::write_message(&mut stream, &request)?;
    let response = st_ipc::read_message::<_, Response>(&mut stream)?;
    Ok(response)
}

/// Canonical key of the currently focused window, or `None`.
fn focused_window_key() -> Option<AppKey> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId,
        };

        // SAFETY: plain Win32 queries; handles are closed on all paths.
        unsafe {
            let hwnd: HWND = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return None;
            }
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = vec![0u16; 32_768];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut len,
            );
            let _ = windows::Win32::Foundation::CloseHandle(handle);
            if ok.is_err() {
                return None;
            }
            buf.truncate(len as usize);
            let path = String::from_utf16_lossy(&buf);
            Some(AppKey::windows_exe(&path))
        }
    }

    #[cfg(not(windows))]
    {
        None
    }
}

/// Screen rect of the focused window, defaulting to the full screen.
fn focused_window_rect() -> (i32, i32, i32, i32) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        unsafe {
            let hwnd = GetForegroundWindow();
            let mut r = windows::Win32::Foundation::RECT::default();
            if windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut r).is_ok() {
                let w = r.right - r.left;
                let h = r.bottom - r.top;
                if w > 0 && h > 0 {
                    return (r.left, r.top, w, h);
                }
            }
        }
        (0, 0, 0, 0) // fallback: caller treats zero as "no overlay"
    }

    #[cfg(not(windows))]
    {
        (0, 0, 0, 0)
    }
}

/// Match a stored `kind:value` app key against a focused app key. Compares the
/// canonical forms; falls back to comparing basenames so a path difference
/// (e.g. the focused path resolving differently than the stored one) does not
/// silently defeat the overlay.
fn app_key_matches(stored: &str, focused: &AppKey) -> bool {
    if let Some(stored_key) = AppKey::parse_db_string(stored) {
        if stored_key == *focused {
            return true;
        }
    }
    // Fallback: basename comparison.
    let stored_basename = stored
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(stored)
        .trim_start_matches("win-exe:");
    stored_basename.eq_ignore_ascii_case(focused.basename())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_key_matches() {
        let focused = AppKey::windows_exe("C:\\Games\\Elden Ring\\game\\eldenring.exe");
        assert!(app_key_matches(
            "win-exe:c:\\games\\elden ring\\game\\eldenring.exe",
            &focused
        ));
    }

    #[test]
    fn basename_fallback_matches() {
        let focused = AppKey::windows_exe("C:\\somewhere\\ELDENRING.exe");
        assert!(app_key_matches("win-exe:eldenring.exe", &focused));
        assert!(!app_key_matches("win-exe:notepad.exe", &focused));
    }

    #[test]
    fn non_matching_keys_are_false() {
        let focused = AppKey::windows_exe("C:\\notepad.exe");
        assert!(!app_key_matches("win-exe:c:\\brave.exe", &focused));
    }
}
