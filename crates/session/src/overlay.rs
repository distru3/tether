//! The block overlay: commands the Tauri React transparent overlay via the named pipe bridge.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

/// Programs allowed to serve the overlay bridge (the Tauri UI; Tauri 2 may
/// name the bundled executable after the product).
const UI_EXE: [&str; 2] = ["screentime-ui.exe", "Tether.exe"];

/// How often an acknowledged overlay is re-asserted while the block holds.
/// Re-sending is what makes the block screen self-healing: Alt+F4 on the
/// overlay, a UI crash or restart, or a dropped message are all repaired
/// within this interval. `Show` is idempotent on the UI side.
const RESEND_EVERY: Duration = Duration::from_secs(2);
/// Backoff while unacknowledged (UI not running yet, or starting up).
const RETRY_MIN: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(10);
/// Never spawn the UI more often than this.
const LAUNCH_EVERY: Duration = Duration::from_secs(15);

/// Shared between the session loop and the short-lived sender threads.
#[derive(Default)]
struct Delivery {
    acked: AtomicBool,
    in_flight: AtomicBool,
    last_launch: Mutex<Option<Instant>>,
}

/// Everything one overlay run owns, held by the session loop until teardown.
pub struct OverlayRun {
    request: st_ipc::OverlayBridgeRequest,
    delivery: Arc<Delivery>,
    last_sent: Instant,
    misses: u32,
}

impl OverlayRun {
    /// Re-send `Show` when due. Call every cycle while the block holds; cheap
    /// when nothing is due.
    pub fn maintain(&mut self) {
        if self.delivery.in_flight.load(Ordering::Acquire) {
            return;
        }
        let acked = self.delivery.acked.load(Ordering::Acquire);
        let due = if acked {
            self.misses = 0;
            RESEND_EVERY
        } else {
            self.misses = self.misses.saturating_add(1);
            (RETRY_MIN * 2u32.saturating_pow(self.misses.min(4))).min(RETRY_MAX)
        };
        if self.last_sent.elapsed() >= due {
            self.send();
        }
    }

    fn send(&mut self) {
        self.last_sent = Instant::now();
        self.delivery.in_flight.store(true, Ordering::Release);
        let request = self.request.clone();
        let delivery = Arc::clone(&self.delivery);
        let spawned = std::thread::Builder::new()
            .name("overlay-send".into())
            .spawn(move || {
                let acked = deliver(&request, &delivery);
                delivery.acked.store(acked, Ordering::Release);
                delivery.in_flight.store(false, Ordering::Release);
            });
        if spawned.is_err() {
            self.delivery.in_flight.store(false, Ordering::Release);
        }
    }

    /// Dismiss the overlay and send Hide over the bridge.
    pub fn dismiss_and_join(self) {
        if let Ok(mut stream) = st_ipc::transport::client_connect(st_ipc::OVERLAY_PIPE_NAME) {
            if server_is_ui(&stream) {
                let _ = st_ipc::write_message(&mut stream, &st_ipc::OverlayBridgeRequest::Hide);
                let _ = st_ipc::read_message::<_, st_ipc::OverlayBridgeResponse>(&mut stream);
            }
        }
    }
}

/// One delivery attempt: connect (launching the UI if it is not running),
/// check the server is really Tether's UI, send, and wait for the Ack.
fn deliver(request: &st_ipc::OverlayBridgeRequest, delivery: &Delivery) -> bool {
    let mut stream = match st_ipc::transport::client_connect(st_ipc::OVERLAY_PIPE_NAME) {
        Ok(stream) => stream,
        Err(_) => {
            let mut last = delivery
                .last_launch
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if last.is_none_or(|t| t.elapsed() >= LAUNCH_EVERY) {
                *last = Some(Instant::now());
                drop(last);
                tracing::info!("overlay bridge unreachable; launching the UI");
                try_launch_ui();
            }
            return false;
        }
    };
    if !server_is_ui(&stream) {
        return false;
    }
    if st_ipc::write_message(&mut stream, request).is_err() {
        return false;
    }
    matches!(
        st_ipc::read_message::<_, st_ipc::OverlayBridgeResponse>(&mut stream),
        Ok(st_ipc::OverlayBridgeResponse::Ack)
    )
}

/// Refuse to talk to a bridge served by anything but Tether's UI: another
/// process could create the pipe first and swallow every `Show`. An
/// uninspectable server is given the benefit of the doubt (logged).
fn server_is_ui(stream: &st_ipc::transport::PipeStream) -> bool {
    #[cfg(windows)]
    match st_win32::peer_is_trusted(stream.peer_process_id(), &UI_EXE) {
        Some(false) => {
            tracing::warn!("overlay bridge is served by an unknown process; not sending");
            return false;
        }
        None => tracing::debug!("overlay bridge server not inspectable"),
        Some(true) => {}
    }
    #[cfg(not(windows))]
    let _ = (stream, &UI_EXE);
    true
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
    let (x, y, width, height) = rect;
    let request = st_ipc::OverlayBridgeRequest::Show {
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
    let mut run = OverlayRun {
        request,
        delivery: Arc::new(Delivery::default()),
        last_sent: Instant::now(),
        misses: 0,
    };
    run.send();
    run
}

fn try_launch_ui() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let mut candidates = Vec::new();
                for base in [Some(dir), dir.parent()].into_iter().flatten() {
                    for name in UI_EXE {
                        candidates.push(base.join(name));
                    }
                }
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
