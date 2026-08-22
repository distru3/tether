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
//! * Topmost, borderless, layered window over the blocked app's rect. Opaque,
//!   so mouse input cannot pass through.
//! * Fades in quickly (opacity 0 → 1 over ~200 ms).
//! * A low-level keyboard hook swallows keys while visible, so the blocked app
//!   cannot be driven (Alt+Tab, Alt+F4, shortcuts). The overlay has no close
//!   affordance and cannot be dismissed.
//! * If a PIN is configured, a PIN field gates the Quit / +15 min buttons: a
//!   wrong PIN keeps the overlay locked.
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
    GetWindowLongPtrW, GetWindowTextW, PostQuitMessage, RegisterClassExW, SetWindowLongPtrW,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, BS_PUSHBUTTON, ES_AUTOHSCROLL,
    ES_PASSWORD, GWLP_USERDATA, HHOOK, MSG, WINDOWS_HOOK_ID, WINDOW_STYLE, WM_COMMAND, WM_DESTROY,
    WM_NCCREATE, WNDCLASSEXW, WS_CHILD, WS_EX_LAYERED, WS_EX_TOPMOST, WS_POPUP, WS_TABSTOP,
    WS_VISIBLE,
};

/// `WH_KEYBOARD_LL` is a `WINDOWS_HOOK_ID` constant in `WindowsAndMessaging`.
const WH_KEYBOARD_LL: WINDOWS_HOOK_ID = WINDOWS_HOOK_ID(13);

/// Child control IDs.
const BTN_QUIT: i32 = 1;
const BTN_EXTEND: i32 = 2;
const EDIT_PIN: i32 = 3;

/// Callbacks the session loop provides so the overlay can trigger agent
/// actions (Quit / +15 min). Each returns whether the action was accepted; a
/// `false` means the PIN was wrong and the overlay stays locked.
pub struct OverlayCallbacks {
    pub on_quit: std::sync::Arc<dyn Fn(&str) -> bool + Send + Sync>,
    pub on_extend: std::sync::Arc<dyn Fn(&str) -> bool + Send + Sync>,
}

impl OverlayCallbacks {
    pub fn new<Q, E>(on_quit: Q, on_extend: E) -> Self
    where
        Q: Fn(&str) -> bool + Send + Sync + 'static,
        E: Fn(&str) -> bool + Send + Sync + 'static,
    {
        Self {
            on_quit: std::sync::Arc::new(on_quit),
            on_extend: std::sync::Arc::new(on_extend),
        }
    }
}

/// The low-level keyboard hook, installed only while the overlay is visible.
/// Stored as its pointer address so the `static` stays `Send + Sync`.
static KEYBOARD_HOOK: std::sync::Mutex<Option<isize>> = std::sync::Mutex::new(None);

/// Shared control for the session loop: lets the poll thread dismiss the
/// overlay (e.g. when the block lifts) by posting `WM_CLOSE` to its window.
/// Stores the window handle as its pointer address so the struct stays `Send`.
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

    /// Ask the overlay to close. Safe to call from any thread; it posts a
    /// `WM_CLOSE` which the message loop processes.
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
/// in a message loop until the overlay is dismissed. Call on a dedicated
/// thread so the polling loop can keep watching the agent.
pub fn run_overlay(
    rect: (i32, i32, i32, i32),
    callbacks: OverlayCallbacks,
    control: Arc<OverlayControl>,
) {
    unsafe { run_overlay_impl(rect, callbacks, control) }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            // `lparam` points at the CREATESTRUCT whose `lpCreateParams` is the
            // callbacks we passed to CreateWindowExW. Store it for later.
            let cs = lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW;
            let callbacks = (*cs).lpCreateParams as *const OverlayCallbacks;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, callbacks as isize);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xffff) as i32;
            let callbacks = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const OverlayCallbacks;
            if !callbacks.is_null() {
                let cb = &*callbacks;
                let pin = read_pin(hwnd);
                let accepted = match id {
                    BTN_QUIT => (cb.on_quit)(&pin),
                    BTN_EXTEND => (cb.on_extend)(&pin),
                    _ => false,
                };
                if accepted {
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let callbacks = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OverlayCallbacks;
            if !callbacks.is_null() {
                drop(Box::from_raw(callbacks));
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn read_pin(hwnd: HWND) -> String {
    let edit = pin_edit_handle(hwnd);
    let mut buf = vec![0u16; 64];
    let len = GetWindowTextW(edit, &mut buf);
    if len > 0 {
        String::from_utf16_lossy(&buf[..len as usize])
    } else {
        String::new()
    }
}

fn pin_edit_handle(hwnd: HWND) -> HWND {
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, EDIT_PIN) }
        .unwrap_or(HWND::default())
}

unsafe fn run_overlay_impl(
    rect: (i32, i32, i32, i32),
    callbacks: OverlayCallbacks,
    control: Arc<OverlayControl>,
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
    let cb = Box::into_raw(Box::new(callbacks));
    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOPMOST,
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
        Some(cb as *const c_void),
    );
    let Ok(hwnd) = hwnd else {
        drop(unsafe { Box::from_raw(cb) });
        tracing::error!("failed to create overlay window");
        return;
    };

    control.register(hwnd);
    create_controls(hwnd, hinstance.into());

    // Fade in over ~200 ms: step +25 alpha every 20 ms until fully opaque.
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

unsafe fn create_controls(hwnd: HWND, hinstance: windows::Win32::Foundation::HINSTANCE) {
    // Buttons and the PIN field, centered in the overlay. The control id goes
    // in the `hmenu` parameter of CreateWindowExW (that is how Win32 identifies
    // child controls in WM_COMMAND).
    let cw = 200;
    let ch = 40;
    let cx = 24;
    let cy = h_center(hwnd);
    let btn_style =
        |extra: i32| WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | extra as u32);

    let _ = CreateWindowExW(
        windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
        PCWSTR(w!("BUTTON").as_ptr()),
        PCWSTR(w!("Quit").as_ptr()),
        btn_style(BS_PUSHBUTTON),
        cx,
        cy,
        cw,
        ch,
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(BTN_QUIT as _),
        hinstance,
        None,
    );
    let _ = CreateWindowExW(
        windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
        PCWSTR(w!("BUTTON").as_ptr()),
        PCWSTR(w!("+15 minutes").as_ptr()),
        btn_style(BS_PUSHBUTTON),
        cx + cw + 12,
        cy,
        cw,
        ch,
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(BTN_EXTEND as _),
        hinstance,
        None,
    );
    let _ = CreateWindowExW(
        windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
        PCWSTR(w!("EDIT").as_ptr()),
        PCWSTR(w!("").as_ptr()),
        btn_style(ES_PASSWORD | ES_AUTOHSCROLL),
        cx,
        cy - 56,
        cw * 2 + 12,
        32,
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(EDIT_PIN as _),
        hinstance,
        None,
    );
}

unsafe fn h_center(hwnd: HWND) -> i32 {
    let mut r = windows::Win32::Foundation::RECT::default();
    let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut r);
    (r.bottom - r.top) / 2 - 20
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
