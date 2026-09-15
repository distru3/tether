//! The block overlay: commands the Tauri React transparent overlay via the named pipe bridge.

use std::sync::Arc;

/// Which actions the overlay shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    /// No PIN configured: show Quit + +15 min.
    Buttons,
    /// PIN configured: show the PIN pad + extend + Quit button.
    PinExtend,
}

/// Callbacks into the session shell.
#[allow(dead_code)]
pub struct OverlayCallbacks {
    pub on_quit: Arc<dyn Fn() -> bool + Send + Sync>,
    pub on_extend: Arc<dyn Fn(String) -> bool + Send + Sync>,
}

impl OverlayCallbacks {
    pub fn new<Q, E>(on_quit: Q, on_extend: E) -> Self
    where
        Q: Fn() -> bool + Send + Sync + 'static,
        E: Fn(String) -> bool + Send + Sync + 'static,
    {
        Self {
            on_quit: Arc::new(on_quit),
            on_extend: Arc::new(on_extend),
        }
    }
}

/// Everything one overlay run owns, held by the session loop until teardown.
pub struct OverlayRun {
    #[allow(dead_code)]
    target_hwnd: isize,
}

impl OverlayRun {
    /// Dismiss the overlay and send Hide over the bridge.
    pub fn dismiss_and_join(self) {
        if let Ok(mut stream) = st_ipc::transport::client_connect(st_ipc::OVERLAY_PIPE_NAME) {
            let _ = st_ipc::write_message(&mut stream, &st_ipc::OverlayBridgeRequest::Hide);
            let _ = st_ipc::read_message::<_, st_ipc::OverlayBridgeResponse>(&mut stream);
        }
    }
}

/// Spawns the overlay by dispatching Show across the bridge to the Tauri host.
pub fn spawn_overlay(
    app_id: i64,
    pid: u32,
    target_hwnd: isize,
    rect: (i32, i32, i32, i32),
    _callbacks: OverlayCallbacks,
    mode: OverlayMode,
    label: String,
) -> OverlayRun {
    // Dispatch Show across the overlay bridge to the Tauri host
    let (x, y, width, height) = rect;
    let req = st_ipc::OverlayBridgeRequest::Show {
        app_id,
        label,
        pin_locked: mode == OverlayMode::PinExtend,
        target_hwnd: Some(target_hwnd as i64),
        process_id: pid,
        rect: Some(st_ipc::OverlayRectDto {
            x,
            y,
            width,
            height,
        }),
    };

    std::thread::spawn(move || {
        if let Ok(mut stream) = st_ipc::transport::client_connect(st_ipc::OVERLAY_PIPE_NAME) {
            let _ = st_ipc::write_message(&mut stream, &req);
            let _ = st_ipc::read_message::<_, st_ipc::OverlayBridgeResponse>(&mut stream);
        } else {
            // Attempt to wake/spawn UI process if not running
            try_launch_ui();
            std::thread::sleep(std::time::Duration::from_millis(200));
            if let Ok(mut stream) = st_ipc::transport::client_connect(st_ipc::OVERLAY_PIPE_NAME) {
                let _ = st_ipc::write_message(&mut stream, &req);
                let _ = st_ipc::read_message::<_, st_ipc::OverlayBridgeResponse>(&mut stream);
            }
        }
    });

    OverlayRun { target_hwnd }
}

fn try_launch_ui() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidates = [
                    dir.join("screentime-ui.exe"),
                    dir.join("../screentime-ui.exe"),
                    dir.join("../../screentime-ui.exe"),
                ];
                for candidate in &candidates {
                    if candidate.exists() {
                        let _ = std::process::Command::new(candidate)
                            .creation_flags(0x08000000) // CREATE_NO_WINDOW
                            .spawn();
                        break;
                    }
                }
            }
        }
    }
}
