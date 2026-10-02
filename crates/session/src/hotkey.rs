//! Global "peek the HUD" hotkey: parsing and the RegisterHotKey message pump.

#[cfg(windows)]
pub(crate) const WM_HOTKEY_UPDATE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_USER + 10;

#[cfg(windows)]
pub(crate) const HOTKEY_ID: i32 = 0x5448; // "TH" for Tether HUD

#[cfg(windows)]
pub(crate) fn parse_hotkey(
    s: &str,
) -> Option<(
    windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS,
    u32,
)> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    };

    let mut mods = MOD_NOREPEAT;
    let mut vk: Option<u32> = None;

    for part in s.split('+') {
        let trimmed = part.trim();
        let upper = trimmed.to_ascii_uppercase();
        match upper.as_str() {
            "CTRL" | "CONTROL" => mods |= MOD_CONTROL,
            "ALT" => mods |= MOD_ALT,
            "SHIFT" => mods |= MOD_SHIFT,
            "WIN" | "WINDOWS" | "SUPER" | "META" => mods |= MOD_WIN,
            "F1" => vk = Some(0x70),
            "F2" => vk = Some(0x71),
            "F3" => vk = Some(0x72),
            "F4" => vk = Some(0x73),
            "F5" => vk = Some(0x74),
            "F6" => vk = Some(0x75),
            "F7" => vk = Some(0x76),
            "F8" => vk = Some(0x77),
            "F9" => vk = Some(0x78),
            "F10" => vk = Some(0x79),
            "F11" => vk = Some(0x7A),
            "F12" => vk = Some(0x7B),
            "TAB" => vk = Some(0x09),
            "SPACE" => vk = Some(0x20),
            "\\" => vk = Some(0xDC),
            "/" => vk = Some(0xBF),
            "`" => vk = Some(0xC0),
            "-" => vk = Some(0xBD),
            "=" => vk = Some(0xBB),
            "[" => vk = Some(0xDB),
            "]" => vk = Some(0xDD),
            "'" => vk = Some(0xDE),
            ";" => vk = Some(0xBA),
            "," => vk = Some(0xBC),
            "." => vk = Some(0xBE),
            other if other.len() == 1 => {
                let c = other.chars().next().unwrap();
                if c.is_ascii_alphanumeric() {
                    vk = Some(c as u32);
                }
            }
            _ => {}
        }
    }

    vk.map(|k| (mods, k))
}

#[cfg(windows)]
pub(crate) struct HotkeyManager {
    update_tx: std::sync::mpsc::Sender<String>,
    last_pressed: std::sync::Arc<std::sync::atomic::AtomicU64>,
    thread_id: u32,
}

#[cfg(windows)]
impl HotkeyManager {
    pub(crate) fn spawn(initial_hotkey: String) -> Self {
        use windows::Win32::System::Threading::GetCurrentThreadId;
        use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, PeekMessageW, TranslateMessage, MSG, PM_NOREMOVE,
            WM_HOTKEY,
        };

        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (update_tx, update_rx) = std::sync::mpsc::channel::<String>();
        let last_pressed = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let last_pressed_clone = last_pressed.clone();

        std::thread::spawn(move || unsafe {
            let tid = GetCurrentThreadId();
            // Force Windows to instantiate the thread's message queue
            let mut dummy = MSG::default();
            let _ = PeekMessageW(&mut dummy, None, 0, 0, PM_NOREMOVE);
            let _ = ready_tx.send(tid);

            let mut current_hotkey = initial_hotkey;
            if let Some((mods, vk)) = parse_hotkey(&current_hotkey) {
                let _ = RegisterHotKey(None, HOTKEY_ID, mods, vk);
            }

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_HOTKEY && msg.wParam.0 as i32 == HOTKEY_ID {
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    last_pressed_clone.store(now_ms, std::sync::atomic::Ordering::Relaxed);
                } else if msg.message == WM_HOTKEY_UPDATE {
                    while let Ok(new_key) = update_rx.try_recv() {
                        if new_key != current_hotkey {
                            let _ = UnregisterHotKey(None, HOTKEY_ID);
                            if let Some((mods, vk)) = parse_hotkey(&new_key) {
                                let _ = RegisterHotKey(None, HOTKEY_ID, mods, vk);
                            }
                            current_hotkey = new_key;
                        }
                    }
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let _ = UnregisterHotKey(None, HOTKEY_ID);
        });

        let thread_id = ready_rx.recv().unwrap_or(0);
        Self {
            update_tx,
            last_pressed,
            thread_id,
        }
    }

    pub(crate) fn update_hotkey(&self, new_hotkey: String) {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
        let _ = self.update_tx.send(new_hotkey);
        if self.thread_id != 0 {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_HOTKEY_UPDATE, WPARAM(0), LPARAM(0));
            }
        }
    }

    pub(crate) fn was_pressed_recently(&self, within_ms: u64) -> bool {
        let last = self.last_pressed.load(std::sync::atomic::Ordering::Relaxed);
        if last == 0 {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        now.saturating_sub(last) <= within_ms
    }

    pub(crate) fn peek_remaining_ms(&self, duration_ms: u64) -> Option<u64> {
        let last = self.last_pressed.load(std::sync::atomic::Ordering::Relaxed);
        if last == 0 {
            return None;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let elapsed = now.saturating_sub(last);
        if elapsed < duration_ms {
            Some(duration_ms - elapsed)
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
pub(crate) struct HotkeyManager;

#[cfg(not(windows))]
impl HotkeyManager {
    pub(crate) fn was_pressed_recently(&self, _within_ms: u64) -> bool {
        false
    }

    pub(crate) fn peek_remaining_ms(&self, _duration_ms: u64) -> Option<u64> {
        None
    }
}

// ---------------------------------------------------------------------------
// Startup plumbing: single-instance naming + two-layer logging.
//
// Mirrors the agent's setup on purpose: same EnvFilter semantics, same
// daily-rolling shape, same WorkerGuard lifetime rule. The two binaries stay
// deliberately independent (each owns its wiring), which is why this is a
// small copy rather than shared code — st-win32 is Win32 plumbing, not a
// logging facade.
// ---------------------------------------------------------------------------
