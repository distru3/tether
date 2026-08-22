//! The block overlay: a topmost, borderless, input-blocking window.
//!
//! # Why this exists
//!
//! M2.5 replaced process freezing with a non-invasive overlay. The overlay is
//! the only mechanism that works for anti-cheat-protected or elevated apps
//! (which refuse `NtSuspendProcess`) and the only one that can present the
//! user a choice — Quit or extend. It lives in this per-user helper because
//! the agent (SYSTEM, Session 0) cannot draw on the interactive desktop.
//!
//! # Behaviour
//!
//! * A single topmost, borderless, layered window over the blocked app's rect.
//!   It is opaque, so mouse input cannot pass through.
//! * Fully custom-painted in `WM_PAINT` (no child controls): a dark panel, the
//!   app label, and hit-region buttons (Quit / +15 min). Because there are no
//!   child windows, layered alpha fades work — the overlay genuinely fades in
//!   over ~200 ms.
//! * `WS_EX_NOACTIVATE` keeps the overlay from ever becoming the "foreground
//!   window", so clicking it cannot confuse the session's focus-based logic.
//! * A low-level keyboard hook swallows all keys while the overlay is visible,
//!   so the blocked app cannot be driven (Alt+Tab, Alt+F4, shortcuts). The
//!   overlay has no close affordance and cannot be dismissed.
//! * Two modes:
//!   - [`OverlayMode::Buttons`] (no PIN): show **Quit** and **+15 min**.
//!   - [`OverlayMode::ExtendLocked`] (PIN set): show **Quit** plus a message
//!     telling the user to open Screentime to extend.
//!
//! # Honest limits
//!
//! A truly exclusive-fullscreen game may render the overlay behind it. Most
//! games (including Elden Ring) run borderless, which the overlay covers.

use std::ffi::c_void;
use std::sync::Arc;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, PostQuitMessage, RegisterClassExW, SetWindowLongPtrW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, GWLP_USERDATA, HHOOK, MSG, WINDOWS_HOOK_ID, WM_DESTROY,
    WM_LBUTTONUP, WM_NCCREATE, WM_PAINT, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

/// `WH_KEYBOARD_LL` is a `WINDOWS_HOOK_ID` constant in `WindowsAndMessaging`.
const WH_KEYBOARD_LL: WINDOWS_HOOK_ID = WINDOWS_HOOK_ID(13);

/// Which actions the overlay shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    /// No PIN configured: show Quit + +15 min.
    Buttons,
    /// PIN configured: show Quit + a "open Screentime to extend" message.
    ExtendLocked,
}

/// Callbacks the session loop provides so the overlay can trigger agent
/// actions. Quit is always available; extend only in [`OverlayMode::Buttons`].
pub struct OverlayCallbacks {
    pub on_quit: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
    pub on_extend: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
}

impl OverlayCallbacks {
    pub fn new<Q, E>(on_quit: Q, on_extend: E) -> Self
    where
        Q: Fn() -> bool + Send + Sync + 'static,
        E: Fn() -> bool + Send + Sync + 'static,
    {
        Self {
            on_quit: std::sync::Arc::new(on_quit),
            on_extend: std::sync::Arc::new(on_extend),
        }
    }
}

/// Per-window state stored in `GWLP_USERDATA`.
struct OverlayState {
    callbacks: OverlayCallbacks,
    mode: OverlayMode,
    label: String,
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

/// The low-level keyboard hook, installed only while the overlay is visible.
/// Stored as its pointer address so the `static` stays `Send + Sync`.
static KEYBOARD_HOOK: std::sync::Mutex<Option<isize>> = std::sync::Mutex::new(None);

/// Shared control for the session loop: lets the poll thread dismiss the
/// overlay by posting `WM_CLOSE`. Stores the handle as its pointer address.
pub struct OverlayControl {
    hwnd: std::sync::Mutex<Option<isize>>,
}

impl OverlayControl {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            hwnd: std::sync::Mutex::new(None),
        })
    }

    fn register(&self, hwnd: HWND) {
        *self.hwnd.lock().unwrap() = Some(hwnd.0 as isize);
    }

    pub fn dismiss(&self) {
        if let Some(addr) = *self.hwnd.lock().unwrap() {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    HWND(addr as *mut c_void),
                    windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }
        }
    }
}

/// Run the overlay over `rect` (x, y, width, height). Blocks the calling thread
/// in a message loop until dismissed. Call on a dedicated thread.
pub fn run_overlay(
    rect: (i32, i32, i32, i32),
    callbacks: OverlayCallbacks,
    control: Arc<OverlayControl>,
    mode: OverlayMode,
    label: String,
) {
    unsafe { run_overlay_impl(rect, callbacks, control, mode, label) }
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
        WM_LBUTTONUP => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const OverlayState;
            if !state.is_null() {
                let px = (lparam.0 & 0xffff) as i16 as i32;
                let py = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
                let st = &*state;
                let (qrect, errect) = layout(hwnd, st.mode);
                let mut accepted = false;
                if let Some(r) = qrect {
                    if r.contains(px, py) {
                        accepted = (st.callbacks.on_quit)();
                    }
                }
                if !accepted && st.mode == OverlayMode::Buttons {
                    if let Some(r) = errect {
                        if r.contains(px, py) {
                            accepted = (st.callbacks.on_extend)();
                        }
                    }
                }
                if accepted {
                    let _ = DestroyWindow(hwnd);
                }
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

/// Layout constants — keep in sync with `layout`.
const PAD: i32 = 24;
const BTN_H: i32 = 44;
const BTN_GAP: i32 = 12;

/// Compute button hit-rects from the window's client size. In
/// [`OverlayMode::ExtendLocked`] there is no extend button.
unsafe fn layout(hwnd: HWND, mode: OverlayMode) -> (Option<RectI>, Option<RectI>) {
    let mut rect = windows::Win32::Foundation::RECT::default();
    let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
    let cw = rect.right - rect.left;
    let ch = rect.bottom - rect.top;

    let avail = cw - PAD * 2;
    let btn_w = (avail - BTN_GAP) / 2;
    let by = ch - PAD - BTN_H;

    let qrect = RectI {
        x: PAD,
        y: by,
        w: btn_w,
        h: BTN_H,
    };
    let errect = if mode == OverlayMode::Buttons {
        Some(RectI {
            x: PAD + btn_w + BTN_GAP,
            y: by,
            w: btn_w,
            h: BTN_H,
        })
    } else {
        None
    };
    (Some(qrect), errect)
}

unsafe fn paint(hwnd: HWND, state: &OverlayState) {
    let mut rect = windows::Win32::Foundation::RECT::default();
    let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
    let cw = rect.right - rect.left;

    let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
    let hdc = windows::Win32::Graphics::Gdi::BeginPaint(hwnd, &mut ps);

    // Background: dark ink panel.
    let bg = windows::Win32::Graphics::Gdi::CreateSolidBrush(windows::Win32::Foundation::COLORREF(
        0x0c0f1a,
    ));
    let _ = windows::Win32::Graphics::Gdi::FillRect(hdc, &rect, bg);
    let _ = windows::Win32::Graphics::Gdi::DeleteObject(bg);

    // Top accent line (ember).
    let accent = windows::Win32::Graphics::Gdi::CreateSolidBrush(
        windows::Win32::Foundation::COLORREF(0x3d6bff),
    );
    let accent_rect = windows::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: cw,
        bottom: 3,
    };
    let _ = windows::Win32::Graphics::Gdi::FillRect(hdc, &accent_rect, accent);
    let _ = windows::Win32::Graphics::Gdi::DeleteObject(accent);

    // Title.
    let title: Vec<u16> = "Time's up".encode_utf16().collect();
    let _ = windows::Win32::Graphics::Gdi::TextOutW(hdc, PAD, 34, &title);

    // App label.
    let label: Vec<u16> = state.label.encode_utf16().collect();
    let _ = windows::Win32::Graphics::Gdi::TextOutW(hdc, PAD, 70, &label);

    // Buttons at the bottom.
    let (qrect, errect) = layout(hwnd, state.mode);
    if let Some(q) = qrect {
        draw_button(hdc, &q, "Quit", 0x2b3347);
    }
    if let Some(e) = errect {
        draw_button(hdc, &e, "+15 min", 0x3d6bff);
    }

    let _ = windows::Win32::Graphics::Gdi::EndPaint(hwnd, &ps);
}

unsafe fn draw_button(hdc: windows::Win32::Graphics::Gdi::HDC, r: &RectI, label: &str, fill: u32) {
    let brush =
        windows::Win32::Graphics::Gdi::CreateSolidBrush(windows::Win32::Foundation::COLORREF(fill));
    let rr = windows::Win32::Foundation::RECT {
        left: r.x,
        top: r.y,
        right: r.x + r.w,
        bottom: r.y + r.h,
    };
    let _ = windows::Win32::Graphics::Gdi::FillRect(hdc, &rr, brush);
    let _ = windows::Win32::Graphics::Gdi::DeleteObject(brush);

    let wide: Vec<u16> = label.encode_utf16().collect();
    let text_w = wide.len() as i32 * 8;
    let tx = r.x + (r.w - text_w) / 2;
    let ty = r.y + (r.h - 20) / 2;
    let _ = windows::Win32::Graphics::Gdi::TextOutW(hdc, tx, ty, &wide);
}

unsafe fn run_overlay_impl(
    rect: (i32, i32, i32, i32),
    callbacks: OverlayCallbacks,
    control: Arc<OverlayControl>,
    mode: OverlayMode,
    label: String,
) {
    let hinstance = GetModuleHandleW(None).unwrap();
    let class_name = PCWSTR(w!("ScreentimeBlockOverlay").as_ptr());

    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wnd_proc),
        hInstance: hinstance.into(),
        lpszClassName: class_name,
        ..Default::default()
    };
    RegisterClassExW(&wc);

    let (x, y, w, h) = rect;
    let state = Box::into_raw(Box::new(OverlayState {
        callbacks,
        mode,
        label,
    }));

    // WS_EX_NOACTIVATE: clicking the overlay must not make it the foreground
    // window, or the session's focus-based dismissal would loop.
    let hwnd = CreateWindowExW(
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
    );
    let Ok(hwnd) = hwnd else {
        drop(Box::from_raw(state));
        tracing::error!("failed to create overlay window");
        return;
    };

    control.register(hwnd);

    // Fade in over ~200 ms: step +25 alpha every 20 ms. Works now because the
    // window has no child controls.
    let mut alpha: u16 = 0;
    while alpha < 255 {
        alpha = (alpha + 25).min(255);
        let _ = windows::Win32::UI::WindowsAndMessaging::SetLayeredWindowAttributes(
            hwnd,
            windows::Win32::Foundation::COLORREF(0),
            alpha as u8,
            windows::Win32::UI::WindowsAndMessaging::LAYERED_WINDOW_ATTRIBUTES_FLAGS(0x2),
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    install_keyboard_hook();

    // Message loop: runs until the window is destroyed.
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    remove_keyboard_hook();
}

fn install_keyboard_hook() {
    let mut guard = KEYBOARD_HOOK.lock().unwrap();
    if guard.is_some() {
        return;
    }
    if let Ok(hook) = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) } {
        *guard = Some(hook.0 as isize);
    }
}

fn remove_keyboard_hook() {
    let mut guard = KEYBOARD_HOOK.lock().unwrap();
    if let Some(addr) = guard.take() {
        unsafe {
            let _ = UnhookWindowsHookEx(HHOOK(addr as *mut c_void));
        }
    }
}

/// Swallow all keys while the overlay is up, so the blocked app cannot be
/// driven by keyboard (Alt+F4, Alt+Tab, typing, shortcuts).
unsafe extern "system" fn keyboard_proc(_code: i32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
    LRESULT(1)
}
