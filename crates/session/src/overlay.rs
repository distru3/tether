//! The block overlay: a topmost, borderless, input-blocking window.
//!
//! # Why this exists
//!
//! The overlay is the only mechanism that works for anti-cheat-protected or
//! elevated apps (which refuse process freezing) and the only one that can
//! present the user a choice. It lives in this per-user helper because the
//! agent (SYSTEM, Session 0) cannot draw on the interactive desktop.
//!
//! # Modes, and why Quit is not always shown
//!
//! * [`OverlayMode::Buttons`] — no PIN configured. **Quit** and **+15 min**;
//!   both send an empty PIN, which the agent accepts only in this situation.
//! * [`OverlayMode::PinExtend`] — a PIN is configured. There is deliberately
//!   **no Quit button**: its only wire representation is an empty-PIN
//!   `CloseApps`, which the agent would always reject — drawing it would be a
//!   button that lies (audit finding). Instead the overlay shows a PIN pad and
//!   a **+15 min** button that sends the entered PIN via `GrantOverride`.
//!
//! # Why the PIN pad is clicked, not typed
//!
//! The window is `WS_EX_NOACTIVATE`, so it never owns keyboard focus, and the
//! low-level keyboard hook below swallows system-wide keys while blocking.
//! Typed input therefore cannot reach this window even if it wanted it — by
//! design, since those defences are what stop the blocked app from being
//! driven. A click pad works regardless of focus, because the topmost window
//! under the cursor receives mouse input without activation.
//!
//! # Resource ownership (per instance)
//!
//! Audit fix: hooks used to live in a module-global `Mutex<Option<isize>>`,
//! which corrupted when two overlays overlapped (second install skipped,
//! first removal killed the shared hook). Now every overlay run owns its
//! keyboard-hook handle and its window on its own thread, tracked by an
//! [`OverlayRun`] value; teardown posts `WM_CLOSE` and **joins** the thread,
//! so nothing global remains to race.
//!
//! # Behaviour
//!
//! * Opaque custom-painted panel (`WM_PAINT`), hit-region buttons/pad, no
//!   child controls — layered alpha fades stay possible.
//! * Fade-in is driven by a `WM_TIMER` ramp *inside* the message pump (audit
//!   fix: the old code slept ~200 ms before pumping, leaving the window
//!   unpainted). The window now paints immediately at alpha 0 and ramps over
//!   ~200 ms while messages flow.
//! * The keyboard hook swallows all keys while visible (Alt+F4, Alt+Tab,
//!   shortcuts). It is installed after the window exists and removed before
//!   the overlay thread exits.
//! * `WS_EX_NOACTIVATE` keeps the overlay out of the foreground, so clicking
//!   it cannot confuse the session loop's focus-based logic.
//!
//! # Honest limits
//!
//! A truly exclusive-fullscreen game may render the overlay behind it. Most
//! games run borderless, which the overlay covers.

use std::ffi::c_void;
use std::sync::Arc;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, KillTimer, PostMessageW, PostQuitMessage, RegisterClassExW,
    SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, GWLP_USERDATA, LAYERED_WINDOW_ATTRIBUTES_FLAGS, MSG, WINDOWS_HOOK_ID,
    WM_CLOSE, WM_DESTROY, WM_LBUTTONUP, WM_NCCREATE, WM_PAINT, WM_TIMER, WNDCLASSEXW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

/// `WH_KEYBOARD_LL` is a `WINDOWS_HOOK_ID` constant in `WindowsAndMessaging`.
const WH_KEYBOARD_LL: WINDOWS_HOOK_ID = WINDOWS_HOOK_ID(13);

/// Which actions the overlay shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    /// No PIN configured: show Quit + +15 min.
    Buttons,
    /// PIN configured: show the PIN pad + extend; never Quit.
    PinExtend,
}

/// Callbacks into the session shell so clicks become agent requests.
///
/// `on_extend` receives the PIN typed/clicked on the pad (empty when no PIN is
/// configured, which is exactly what makes the no-PIN request succeed).
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
///
/// Why a struct instead of loose fields: dismissal needs the hwnd address the
/// overlay thread registered, and clean shutdown needs to wait until the hook
/// is really gone; pairing them guarantees neither outlives the other.
pub struct OverlayRun {
    control: Arc<OverlayControl>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl OverlayRun {
    /// Ask the overlay to close, then block until its hook is unhooked and
    /// its window gone. Safe to call on an already-dead overlay (join reports
    /// the panic; there is nothing left to leak).
    pub fn dismiss_and_join(mut self) {
        self.control.dismiss();
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                tracing::warn!("overlay thread panicked; resources may have leaked");
            }
        }
    }
}

/// Cross-thread handle onto one overlay's window, used only to post `WM_CLOSE`.
///
/// Per-instance by construction (a fresh one ships with every
/// [`spawn_overlay`]), so overlapping overlays can no longer interfere.
struct OverlayControl {
    hwnd: std::sync::Mutex<Option<isize>>,
}

impl OverlayControl {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            hwnd: std::sync::Mutex::new(None),
        })
    }

    fn register(&self, hwnd: HWND) {
        *self.hwnd.lock().unwrap() = Some(hwnd.0 as isize);
    }

    fn dismiss(&self) {
        if let Some(addr) = *self.hwnd.lock().unwrap() {
            // SAFETY: addr was stored from a live HWND by the overlay thread;
            // after WM_CLOSE the window may be gone, but PostMessageW to a
            // stale hwnd fails harmlessly instead of dereferencing it.
            unsafe {
                let _ = PostMessageW(HWND(addr as *mut c_void), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// Spawn an overlay over `rect` on its own thread.
///
/// Rectangles smaller than the smallest readable panel are grown around their
/// centre (position may drift slightly past screen edges; covering the app
/// matters more than perfect placement).
pub fn spawn_overlay(
    rect: (i32, i32, i32, i32),
    callbacks: OverlayCallbacks,
    mode: OverlayMode,
    label: String,
) -> OverlayRun {
    const MIN_W: i32 = 420;
    const MIN_H: i32 = 460;
    let (x, y, w, h) = rect;
    let (w, h) = (w.max(MIN_W), h.max(MIN_H));
    // Centre the growth so small windows still cover the app's midpoint.
    let x = x + (rect.2 - w) / 2;
    let y = y + (rect.3 - h) / 2;

    let control = OverlayControl::new();
    let thread_control = control.clone();
    let thread = std::thread::Builder::new()
        .name("screentime-overlay".into())
        .spawn(move || run_overlay((x, y, w, h), callbacks, mode, label, thread_control))
        .expect("spawning overlay thread");
    OverlayRun {
        control,
        thread: Some(thread),
    }
}

/// Per-window state living behind `GWLP_USERDATA`; owned by the window itself.
struct OverlayState {
    callbacks: OverlayCallbacks,
    mode: OverlayMode,
    label: String,
    /// Digits clicked so far, rendered masked.
    pin: String,
    /// Set after a rejected PIN until the next edit; repaints the pad in red.
    wrong_pin: bool,
    /// Current layered-window alpha for the fade-in ramp.
    alpha: u8,
}

#[derive(Clone, Copy, Debug)]
struct RectI {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl RectI {
    fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Layout constants — keep in sync with `layout_buttons` / `layout_pad`.
const PAD: i32 = 24;
const BTN_H: i32 = 44;
const BTN_GAP: i32 = 12;
const GRID_COLS: i32 = 3;
const PIN_MAX_DIGITS: usize = 8;

/// Pad labels in hit-order: digits 1–9, clear, 0, enter.
const PAD_KEYS: [&str; 12] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "C", "0", "OK"];

/// Bottom-row buttons in [`OverlayMode::Buttons`].
fn layout_buttons(cw: i32, ch: i32) -> (RectI, RectI) {
    let avail = cw - PAD * 2;
    let btn_w = (avail - BTN_GAP) / 2;
    let by = ch - PAD - BTN_H;
    (
        RectI {
            x: PAD,
            y: by,
            w: btn_w,
            h: BTN_H,
        },
        RectI {
            x: PAD + btn_w + BTN_GAP,
            y: by,
            w: btn_w,
            h: BTN_H,
        },
    )
}

/// Top-left of the pad's key grid for a client size of `cw × ch`.
fn pad_origin(cw: i32, ch: i32) -> (i32, i32) {
    let cell_w = (cw - PAD * 2 - (GRID_COLS - 1) * BTN_GAP) / GRID_COLS;
    let grid_w = cell_w * GRID_COLS + (GRID_COLS - 1) * BTN_GAP;
    let grid_h = 4 * BTN_H + 3 * BTN_GAP;
    ((cw - grid_w) / 2, ch - PAD - grid_h)
}

/// Hit rects for all twelve pad keys, indexed like [`PAD_KEYS`].
fn layout_pad(cw: i32, ch: i32) -> [RectI; 12] {
    let cell_w = (cw - PAD * 2 - (GRID_COLS - 1) * BTN_GAP) / GRID_COLS;
    let (gx, gy) = pad_origin(cw, ch);
    let mut cells = [RectI {
        x: 0,
        y: 0,
        w: cell_w,
        h: BTN_H,
    }; 12];
    for (i, cell) in cells.iter_mut().enumerate() {
        let (i, row) = (i as i32, (i as i32) / GRID_COLS);
        let col = i % GRID_COLS;
        cell.x = gx + col * (cell_w + BTN_GAP);
        cell.y = gy + row * (BTN_H + BTN_GAP);
    }
    cells
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW;
            let state = (*cs).lpCreateParams as *const OverlayState;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_PAINT => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const OverlayState;
            if !state.is_null() {
                paint(hwnd, &*state);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == FADE_TIMER_ID {
                let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OverlayState;
                if !state.is_null() {
                    let st = &mut *state;
                    st.alpha = st.alpha.saturating_add(FADE_ALPHA_STEP);
                    // SAFETY: our own hwnd with the layered flag set at creation.
                    let _ = SetLayeredWindowAttributes(
                        hwnd,
                        windows::Win32::Foundation::COLORREF(0),
                        st.alpha,
                        LAYERED_WINDOW_ATTRIBUTES_FLAGS(0x2), // LWA_ALPHA
                    );
                    if st.alpha == u8::MAX {
                        let _ = KillTimer(hwnd, FADE_TIMER_ID);
                    }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OverlayState;
            if !state.is_null() {
                handle_click(
                    hwnd,
                    &mut *state,
                    (lparam.0 & 0xffff) as i16 as i32,
                    ((lparam.0 >> 16) & 0xffff) as i16 as i32,
                );
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OverlayState;
            if !state.is_null() {
                drop(Box::from_raw(state));
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Dispatch one click at client point `(px, py)`; repaints when state changed.
///
/// # Safety
///
/// `hwnd` must be a live overlay window whose `GWLP_USERDATA` owns `state`,
/// called only from `wnd_proc` on the overlay's own thread.
unsafe fn handle_click(hwnd: HWND, state: &mut OverlayState, px: i32, py: i32) {
    let mut cr = RECT::default();
    let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut cr);
    let (cw, ch) = (cr.right - cr.left, cr.bottom - cr.top);

    match state.mode {
        OverlayMode::Buttons => {
            let (quit, extend) = layout_buttons(cw, ch);
            let mut accepted = false;
            if quit.contains(px, py) {
                accepted = (state.callbacks.on_quit)();
            } else if extend.contains(px, py) {
                // No PIN configured in this mode, hence the empty PIN.
                accepted = (state.callbacks.on_extend)(String::new());
            }
            if accepted {
                let _ = DestroyWindow(hwnd);
            }
        }
        OverlayMode::PinExtend => {
            for (i, cell) in layout_pad(cw, ch).iter().enumerate() {
                if !cell.contains(px, py) {
                    continue;
                }
                match PAD_KEYS[i] {
                    digit @ ("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "0") => {
                        if state.pin.len() < PIN_MAX_DIGITS {
                            state.pin.push_str(digit);
                            state.wrong_pin = false;
                        }
                    }
                    "C" => {
                        state.pin.clear();
                        state.wrong_pin = false;
                    }
                    "OK" => {
                        if state.pin.is_empty() {
                            state.wrong_pin = true;
                        } else if (state.callbacks.on_extend)(state.pin.clone()) {
                            let _ = DestroyWindow(hwnd);
                            return;
                        } else {
                            // Agent refused (wrong PIN): say so, start over.
                            state.pin.clear();
                            state.wrong_pin = true;
                        }
                    }
                    other => tracing::debug!(key = other, "unhandled pad key"),
                }
                break;
            }
            let _ = InvalidateRect(hwnd, None, false);
        }
    }
}

unsafe fn paint(hwnd: HWND, state: &OverlayState) {
    let mut rect = RECT::default();
    let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
    let (cw, ch) = (rect.right - rect.left, rect.bottom - rect.top);

    let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
    let hdc = windows::Win32::Graphics::Gdi::BeginPaint(hwnd, &mut ps);

    // Background: dark ink panel.
    fill(hdc, &rect, 0x0c0f1a);

    // Top accent line.
    fill(
        hdc,
        &RECT {
            left: 0,
            top: 0,
            right: cw,
            bottom: 3,
        },
        0x3d6bff,
    );

    text(hdc, PAD, 34, "Time's up");
    text(hdc, PAD, 70, &state.label);

    match state.mode {
        OverlayMode::Buttons => {
            let (quit, extend) = layout_buttons(cw, ch);
            draw_button(hdc, &quit, "Quit", 0x2b3347);
            draw_button(hdc, &extend, "+15 min", 0x3d6bff);
        }
        OverlayMode::PinExtend => {
            text(hdc, PAD, 106, "Enter the PIN to add 15 minutes");

            let (gx, gy) = pad_origin(cw, ch);
            // Masked entry above the grid; one dot per clicked digit.
            let dots = "\u{25cf}".repeat(state.pin.chars().count());
            text(hdc, gx, gy - BTN_GAP - 30, &dots);
            if state.wrong_pin {
                text(hdc, gx, gy - BTN_GAP - 6, "Incorrect PIN");
            }

            for (i, cell) in layout_pad(cw, ch).iter().enumerate() {
                let (label, colour) = match PAD_KEYS[i] {
                    "OK" => ("+15 min", 0x3d6bff),
                    "C" => ("Clear", 0x2b3347),
                    digit => (digit, 0x2b3347),
                };
                draw_button(hdc, cell, label, colour);
            }
        }
    }

    let _ = windows::Win32::Graphics::Gdi::EndPaint(hwnd, &ps);
}

/// Solid-colour rectangle helper; the brush lives only for this call.
fn fill(hdc: windows::Win32::Graphics::Gdi::HDC, r: &RECT, colour: u32) {
    // SAFETY: GDI object calls with locally created brush, deleted before return.
    unsafe {
        let brush = windows::Win32::Graphics::Gdi::CreateSolidBrush(
            windows::Win32::Foundation::COLORREF(colour),
        );
        let _ = windows::Win32::Graphics::Gdi::FillRect(hdc, r, brush);
        let _ = windows::Win32::Graphics::Gdi::DeleteObject(brush);
    }
}

fn text(hdc: windows::Win32::Graphics::Gdi::HDC, x: i32, y: i32, s: &str) {
    // SAFETY: TextOutW copies from the slice for its duration.
    unsafe {
        let wide: Vec<u16> = s.encode_utf16().collect();
        let _ = windows::Win32::Graphics::Gdi::TextOutW(hdc, x, y, &wide);
    }
}

fn draw_button(hdc: windows::Win32::Graphics::Gdi::HDC, r: &RectI, label: &str, colour: u32) {
    fill(
        hdc,
        &RECT {
            left: r.x,
            top: r.y,
            right: r.x + r.w,
            bottom: r.y + r.h,
        },
        colour,
    );
    // Crude centring: monospace-ish estimate, consistent with the rest of the
    // hand-painted panel (no child controls, no text metrics plumbing).
    let text_w = label.encode_utf16().count() as i32 * 8;
    text(hdc, r.x + (r.w - text_w) / 2, r.y + (r.h - 20) / 2, label);
}

/// Timer id for the fade-in ramp. Arbitrary but unique within the window.
const FADE_TIMER_ID: usize = 1;
/// Ramp pacing: ~200 ms total (10 steps × 20 ms), matching the old behaviour.
const FADE_STEP_MS: u32 = 20;
const FADE_ALPHA_STEP: u8 = 25;

/// Run one overlay to completion. Blocks the calling thread in a message pump
/// until the window is destroyed; call via [`spawn_overlay`] only.
fn run_overlay(
    rect: (i32, i32, i32, i32),
    callbacks: OverlayCallbacks,
    mode: OverlayMode,
    label: String,
    control: Arc<OverlayControl>,
) {
    // SAFETY: standard Win32 window lifecycle; see inline notes on each step.
    unsafe {
        let hinstance = GetModuleHandleW(None).expect("module handle");
        let class_name = PCWSTR(w!("ScreentimeBlockOverlay").as_ptr());

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        // Failure here means "already registered" (a previous overlay ran in
        // this process); either way the class exists, which is all we need.
        RegisterClassExW(&wc);

        let (x, y, w, h) = rect;
        let state = Box::into_raw(Box::new(OverlayState {
            callbacks,
            mode,
            label,
            pin: String::new(),
            wrong_pin: false,
            alpha: 0,
        }));

        // WS_EX_NOACTIVATE: clicking the overlay must not make it the
        // foreground window, or the session's focus-based dismissal loops.
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            class_name,
            PCWSTR(w!("Blocked").as_ptr()),
            WS_POPUP | WS_VISIBLE,
            x,
            y,
            w,
            h,
            None,
            None,
            hinstance,
            Some(state as *const c_void),
        ) else {
            drop(Box::from_raw(state));
            tracing::error!("failed to create overlay window");
            return;
        };
        control.register(hwnd);

        // Start fully transparent so the first paint lands invisible; the
        // WM_TIMER ramp below then fades us in while the pump runs — no more
        // sleeping before the pump (the window used to sit unpainted ~200ms).
        let _ = SetLayeredWindowAttributes(
            hwnd,
            windows::Win32::Foundation::COLORREF(0),
            0,
            LAYERED_WINDOW_ATTRIBUTES_FLAGS(0x2), // LWA_ALPHA
        );

        // Swallow all keys while the overlay is up. Owned by this stack frame:
        // two overlapping overlays each get their own hook, and removal below
        // cannot touch anyone else's (the old module-global hook could).
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0).ok();
        if hook.is_none() {
            tracing::warn!("keyboard hook unavailable; overlay shows without input blocking");
        }

        SetTimer(hwnd, FADE_TIMER_ID, FADE_STEP_MS, None);

        // Message loop: runs until the window is destroyed.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Teardown for everything this run owns; the session loop joins the
        // thread afterwards, so by the time it proceeds the desktop is quiet.
        let _ = KillTimer(hwnd, FADE_TIMER_ID);
        if let Some(hook) = hook {
            let _ = UnhookWindowsHookEx(hook);
        }
    }
}

/// Swallow all keys while the overlay is up, so the blocked app cannot be
/// driven by keyboard (Alt+F4, Alt+Tab, typing, shortcuts).
unsafe extern "system" fn keyboard_proc(_code: i32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
    LRESULT(1)
}
