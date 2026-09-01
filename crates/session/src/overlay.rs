//! The block overlay: a topmost, borderless, input-blocking window.
//!
//! # Why this exists
//!
//! The overlay is the only mechanism that works for anti-cheat-protected or
//! elevated apps (which refuse process freezing) and the only one that can
//! present the user a choice. It lives in this per-user helper because the
//! agent (SYSTEM, Session 0) cannot draw on the interactive desktop.
//!
//! # Full Window Coverage & Translucent Layering
//!
//! The overlay window covers the entire target application geometry. A deep
//! translucent backdrop layer prevents any mouse interaction with the
//! underlying application, while a centered floating Obsidian card displays
//! the time limit status, options to extend, and a Quit button.
//!
//! # Modes
//!
//! * [`OverlayMode::Buttons`] — no PIN configured. Shows **Quit App** and **+15 min**;
//!   both send an empty PIN, which the agent accepts only in this situation.
//! * [`OverlayMode::PinExtend`] — a PIN is configured. Shows a PIN keypad to
//!   authorize **+15 min**, as well as a **Quit App** button to terminate the app.
//!
//! # Why the PIN pad is clicked, not typed
//!
//! The window is `WS_EX_NOACTIVATE`, so it never owns keyboard focus, and the
//! low-level keyboard hook below swallows system-wide keys while blocking.
//! Typed input therefore cannot reach this window even if it wanted it — by
//! design, since those defences are what stop the blocked app from being
//! driven. A click pad works regardless of focus, because the topmost window
//! under the cursor receives mouse input without activation.

use std::ffi::c_void;
use std::sync::Arc;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, Ellipse, ExtTextOutW, FillRect,
    GetTextExtentPoint32W, InvalidateRect, RoundRect, SelectObject, SetBkMode, SetTextColor,
    ETO_OPTIONS, HDC, HFONT, HGDIOBJ, PS_NULL, PS_SOLID, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowLongPtrW, KillTimer, LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassExW,
    SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, GWLP_USERDATA, IDC_ARROW, LAYERED_WINDOW_ATTRIBUTES_FLAGS, MSG,
    WINDOWS_HOOK_ID, WM_CLOSE, WM_DESTROY, WM_LBUTTONUP, WM_NCCREATE, WM_PAINT, WM_TIMER,
    WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

/// `WH_KEYBOARD_LL` is a `WINDOWS_HOOK_ID` constant in `WindowsAndMessaging`.
const WH_KEYBOARD_LL: WINDOWS_HOOK_ID = WINDOWS_HOOK_ID(13);

/// Which actions the overlay shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    /// No PIN configured: show Quit + +15 min.
    Buttons,
    /// PIN configured: show the PIN pad + extend + Quit button.
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
pub struct OverlayRun {
    control: Arc<OverlayControl>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl OverlayRun {
    /// Ask the overlay to close, then block until its hook is unhooked and
    /// its window gone. Safe to call on an already-dead overlay.
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
            unsafe {
                let _ = PostMessageW(HWND(addr as *mut c_void), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// Card sizing constants.
const CARD_W: i32 = 460;
const CARD_H_BUTTONS: i32 = 360;
const CARD_H_PIN: i32 = 520;
const CORNER_RADIUS: i32 = 16;

/// Spawn an overlay over `rect` on its own thread.
///
/// The window covers the entire app rectangle `(app_x, app_y, app_w, app_h)` so
/// mouse input is fully intercepted, with a centered floating obsidian card.
pub fn spawn_overlay(
    rect: (i32, i32, i32, i32),
    callbacks: OverlayCallbacks,
    mode: OverlayMode,
    label: String,
) -> OverlayRun {
    let ideal_card_h = match mode {
        OverlayMode::Buttons => CARD_H_BUTTONS,
        OverlayMode::PinExtend => CARD_H_PIN,
    };
    let (app_x, app_y, app_w, app_h) = rect;
    // Window must cover at least the application area, and at least the card size + margins.
    let w = app_w.max(CARD_W + 32);
    let h = app_h.max(ideal_card_h + 32);
    let x = if app_w < CARD_W + 32 {
        app_x - (CARD_W + 32 - app_w) / 2
    } else {
        app_x
    };
    let y = if app_h < ideal_card_h + 32 {
        app_y - (ideal_card_h + 32 - app_h) / 2
    } else {
        app_y
    };

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
    /// Set after a rejected PIN until the next edit; repaints the pad with error message.
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

/// Layout constants.
const PAD: i32 = 24;
const BTN_H: i32 = 40;
const BTN_GAP: i32 = 8;
const GRID_COLS: i32 = 3;
const PIN_MAX_DIGITS: usize = 8;

/// Pad labels in hit-order: digits 1–9, clear, 0, enter.
const PAD_KEYS: [&str; 12] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "C", "0", "OK"];

/// Compute the centered card rectangle for client area `(cw, ch)`.
fn card_geometry(cw: i32, ch: i32, mode: OverlayMode) -> RectI {
    let ideal_h = match mode {
        OverlayMode::Buttons => CARD_H_BUTTONS,
        OverlayMode::PinExtend => CARD_H_PIN,
    };
    let card_w = CARD_W.min(cw.saturating_sub(20).max(100));
    let card_h = ideal_h.min(ch.saturating_sub(20).max(100));
    let card_x = (cw - card_w) / 2;
    let card_y = (ch - card_h) / 2;
    RectI {
        x: card_x,
        y: card_y,
        w: card_w,
        h: card_h,
    }
}

/// Bottom-row buttons in [`OverlayMode::Buttons`].
fn layout_buttons(card: &RectI) -> (RectI, RectI) {
    let avail = card.w - PAD * 2;
    let btn_w = (avail - BTN_GAP) / 2;
    let by = card.y + card.h - PAD - BTN_H;
    (
        RectI {
            x: card.x + PAD,
            y: by,
            w: btn_w,
            h: BTN_H,
        },
        RectI {
            x: card.x + PAD + btn_w + BTN_GAP,
            y: by,
            w: btn_w,
            h: BTN_H,
        },
    )
}

/// Top-left of the keypad grid for a card.
fn pad_origin(card: &RectI) -> (i32, i32) {
    let cell_w = (card.w - PAD * 2 - (GRID_COLS - 1) * BTN_GAP) / GRID_COLS;
    let grid_w = cell_w * GRID_COLS + (GRID_COLS - 1) * BTN_GAP;
    let grid_h = 4 * BTN_H + 3 * BTN_GAP;
    let gx = card.x + (card.w - grid_w) / 2;
    // Leave room below for the QUIT button
    let gy = card.y + card.h - PAD - BTN_H - BTN_GAP - grid_h;
    (gx, gy)
}

/// Hit rects for all twelve keypad keys in [`OverlayMode::PinExtend`].
fn layout_pad(card: &RectI) -> [RectI; 12] {
    let cell_w = (card.w - PAD * 2 - (GRID_COLS - 1) * BTN_GAP) / GRID_COLS;
    let (gx, gy) = pad_origin(card);
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

/// Hit rect for the Quit App button in [`OverlayMode::PinExtend`].
fn layout_pin_quit(card: &RectI) -> RectI {
    let (gx, _) = pad_origin(card);
    let cell_w = (card.w - PAD * 2 - (GRID_COLS - 1) * BTN_GAP) / GRID_COLS;
    let grid_w = cell_w * GRID_COLS + (GRID_COLS - 1) * BTN_GAP;
    let by = card.y + card.h - PAD - BTN_H;
    RectI {
        x: gx,
        y: by,
        w: grid_w,
        h: BTN_H,
    }
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
                    let current_alpha = st.alpha.min(TARGET_ALPHA);
                    let _ = SetLayeredWindowAttributes(
                        hwnd,
                        COLORREF(0),
                        current_alpha,
                        LAYERED_WINDOW_ATTRIBUTES_FLAGS(0x2), // LWA_ALPHA
                    );
                    if st.alpha >= TARGET_ALPHA {
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
unsafe fn handle_click(hwnd: HWND, state: &mut OverlayState, px: i32, py: i32) {
    let mut cr = RECT::default();
    let _ = GetClientRect(hwnd, &mut cr);
    let (cw, ch) = (cr.right - cr.left, cr.bottom - cr.top);
    let card = card_geometry(cw, ch, state.mode);

    match state.mode {
        OverlayMode::Buttons => {
            let (quit, extend) = layout_buttons(&card);
            let mut accepted = false;
            if quit.contains(px, py) {
                accepted = (state.callbacks.on_quit)();
            } else if extend.contains(px, py) {
                accepted = (state.callbacks.on_extend)(String::new());
            }
            if accepted {
                let _ = DestroyWindow(hwnd);
            }
        }
        OverlayMode::PinExtend => {
            let quit_btn = layout_pin_quit(&card);
            if quit_btn.contains(px, py) && (state.callbacks.on_quit)() {
                let _ = DestroyWindow(hwnd);
                return;
            }

            for (i, cell) in layout_pad(&card).iter().enumerate() {
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

/// Modern Obsidian & Glass theme palette. COLORREF packs as `0x00bbggrr`.
const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}
const BG_BACKDROP: u32 = rgb(0, 0, 0);
const BG_OBSIDIAN: u32 = rgb(5, 5, 5);
const BG_CARD_ELEVATED: u32 = rgb(17, 17, 17);
const BG_ROSE_BADGE: u32 = rgb(42, 11, 18);
const BORDER_ELEVATED: u32 = rgb(42, 42, 42);
const TEXT_WHITE: u32 = rgb(255, 255, 255);
const TEXT_MUTED: u32 = rgb(153, 153, 153);
const ACCENT_INDIGO: u32 = rgb(99, 102, 241);
const ACCENT_INDIGO_BRIGHT: u32 = rgb(129, 140, 248);
const ACCENT_ROSE: u32 = rgb(244, 63, 94);

/// Font suite for the modern overlay interface.
struct Fonts {
    brand: HFONT,
    badge: HFONT,
    display: HFONT,
    app_label: HFONT,
    caption: HFONT,
    pin_label: HFONT,
    pin_digit: HFONT,
    btn: HFONT,
}

impl Fonts {
    unsafe fn new() -> Self {
        let segoe = PCWSTR(w!("Segoe UI Variable Display").as_ptr());
        let consolas = PCWSTR(w!("Consolas").as_ptr());
        let make = |height: i32, weight: i32, italic: bool, face: PCWSTR| -> HFONT {
            CreateFontW(
                height,
                0,
                0,
                0,
                weight,
                italic as u32,
                0,
                0,
                1, // DEFAULT_CHARSET
                0,
                0,
                5, // CLEARTYPE_QUALITY
                0,
                face,
            )
        };
        Self {
            brand: make(-12, 700, false, consolas),
            badge: make(-11, 700, false, consolas),
            display: make(-24, 700, false, segoe),
            app_label: make(-18, 600, false, segoe),
            caption: make(-13, 400, false, segoe),
            pin_label: make(-11, 700, false, consolas),
            pin_digit: make(-18, 600, false, segoe),
            btn: make(-13, 600, false, segoe),
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        for f in [
            &self.brand,
            &self.badge,
            &self.display,
            &self.app_label,
            &self.caption,
            &self.pin_label,
            &self.pin_digit,
            &self.btn,
        ] {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(f.0));
            }
        }
    }
}

unsafe fn select_font(hdc: HDC, font: &HFONT) -> HGDIOBJ {
    SelectObject(hdc, HGDIOBJ(font.0))
}

/// Measure a UTF-16 string under the given font selection.
unsafe fn text_extent(hdc: HDC, s: &str, font: &HFONT) -> (i32, i32) {
    let old = select_font(hdc, font);
    let mut size = SIZE::default();
    let wide: Vec<u16> = s.encode_utf16().collect();
    let _ = GetTextExtentPoint32W(hdc, &wide, &mut size);
    select_font(hdc, &HFONT(old.0));
    (size.cx, size.cy)
}

/// Truncate to fit `max_w` with a trailing ellipsis, measuring as we cut.
unsafe fn ellipsize(hdc: HDC, s: &str, font: &HFONT, max_w: i32) -> String {
    let (w, _) = text_extent(hdc, s, font);
    if w <= max_w {
        return s.to_string();
    }
    let mut chars: Vec<char> = s.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>() + "\u{2026}";
        let (cw, _) = text_extent(hdc, &candidate, font);
        if cw <= max_w {
            return candidate;
        }
    }
    String::new()
}

/// Draw text at `(x, y)` (top-left) in the given face and colour.
unsafe fn draw_text(hdc: HDC, x: i32, y: i32, s: &str, font: &HFONT, colour: u32) {
    select_font(hdc, font);
    SetTextColor(hdc, COLORREF(colour));
    SetBkMode(hdc, TRANSPARENT);
    let wide: Vec<u16> = s.encode_utf16().collect();
    let _ = ExtTextOutW(
        hdc,
        x,
        y,
        ETO_OPTIONS(0),
        None,
        PCWSTR(wide.as_ptr()),
        wide.len() as u32,
        None,
    );
}

/// Like [`draw_text`] but centred on horizontal position `cx`.
unsafe fn draw_text_centered(hdc: HDC, cx: i32, y: i32, s: &str, font: &HFONT, colour: u32) {
    let (w, _) = text_extent(hdc, s, font);
    draw_text(hdc, cx - w / 2, y, s, font, colour);
}

/// Uppercase text with letter-spacing.
unsafe fn draw_tracked_caps(
    hdc: HDC,
    x: i32,
    y: i32,
    s: &str,
    font: &HFONT,
    colour: u32,
    track: i32,
) {
    select_font(hdc, font);
    SetTextColor(hdc, COLORREF(colour));
    SetBkMode(hdc, TRANSPARENT);
    let upper = s.to_uppercase();
    let wide: Vec<u16> = upper.encode_utf16().collect();
    let mut dx = Vec::with_capacity(wide.len());
    for ch in upper.chars() {
        let (w, _) = text_extent(hdc, &ch.to_string(), font);
        dx.push(w + track);
    }
    let _ = ExtTextOutW(
        hdc,
        x,
        y,
        ETO_OPTIONS(0),
        None,
        PCWSTR(wide.as_ptr()),
        wide.len() as u32,
        Some(dx.as_ptr()),
    );
}

/// Solid rectangle fill.
fn fill(hdc: HDC, r: &RECT, colour: u32) {
    unsafe {
        let brush = CreateSolidBrush(COLORREF(colour));
        let _ = FillRect(hdc, r, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

/// Draw a rounded rectangle with optional border.
unsafe fn draw_round_rect(
    hdc: HDC,
    r: &RectI,
    corner: i32,
    fill_color: u32,
    border_color: Option<u32>,
) {
    let brush = CreateSolidBrush(COLORREF(fill_color));
    let pen = if let Some(bc) = border_color {
        CreatePen(PS_SOLID, 1, COLORREF(bc))
    } else {
        CreatePen(PS_NULL, 0, COLORREF(0))
    };
    let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
    let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));

    let _ = RoundRect(hdc, r.x, r.y, r.x + r.w, r.y + r.h, corner, corner);

    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    let _ = DeleteObject(HGDIOBJ(brush.0));
    let _ = DeleteObject(HGDIOBJ(pen.0));
}

/// Draw a circle with optional border.
unsafe fn draw_circle(
    hdc: HDC,
    cx: i32,
    cy: i32,
    radius: i32,
    fill_color: u32,
    border_color: Option<u32>,
) {
    let brush = CreateSolidBrush(COLORREF(fill_color));
    let pen = if let Some(bc) = border_color {
        CreatePen(PS_SOLID, 1, COLORREF(bc))
    } else {
        CreatePen(PS_NULL, 0, COLORREF(0))
    };
    let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
    let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));

    let _ = Ellipse(
        hdc,
        cx - radius,
        cy - radius,
        cx + radius + 1,
        cy + radius + 1,
    );

    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    let _ = DeleteObject(HGDIOBJ(brush.0));
    let _ = DeleteObject(HGDIOBJ(pen.0));
}

#[derive(Clone, Copy)]
enum BtnStyle {
    Primary,
    Secondary,
}

unsafe fn draw_button(
    hdc: HDC,
    r: &RectI,
    label: &str,
    style: BtnStyle,
    font: &HFONT,
    text_color: u32,
) {
    match style {
        BtnStyle::Primary => {
            draw_round_rect(hdc, r, 10, ACCENT_INDIGO, Some(ACCENT_INDIGO));
            draw_text_centered(
                hdc,
                r.x + r.w / 2,
                r.y + (r.h - text_extent(hdc, label, font).1) / 2,
                label,
                font,
                text_color,
            );
        }
        BtnStyle::Secondary => {
            draw_round_rect(hdc, r, 10, BG_CARD_ELEVATED, Some(BORDER_ELEVATED));
            draw_text_centered(
                hdc,
                r.x + r.w / 2,
                r.y + (r.h - text_extent(hdc, label, font).1) / 2,
                label,
                font,
                text_color,
            );
        }
    }
}

/// Block badge in the upper right.
unsafe fn draw_block_badge(hdc: HDC, right: i32, y: i32, text: &str, fonts: &Fonts) {
    let (tw, th) = text_extent(hdc, text, &fonts.badge);
    let w = tw + 20;
    let h = 24;
    let r = RectI {
        x: right - w,
        y,
        w,
        h,
    };
    draw_round_rect(hdc, &r, 10, BG_ROSE_BADGE, Some(ACCENT_ROSE));
    draw_text_centered(
        hdc,
        r.x + w / 2,
        y + (h - th) / 2,
        text,
        &fonts.badge,
        ACCENT_ROSE,
    );
}

unsafe fn paint(hwnd: HWND, state: &OverlayState) {
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let (cw, ch) = (rect.right - rect.left, rect.bottom - rect.top);

    let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
    let hdc = windows::Win32::Graphics::Gdi::BeginPaint(hwnd, &mut ps);

    unsafe {
        let fonts = Fonts::new();

        // 1. Full-window backdrop: deep translucent veil covering the entire target application.
        fill(
            hdc,
            &RECT {
                left: 0,
                top: 0,
                right: cw,
                bottom: ch,
            },
            BG_BACKDROP,
        );

        // 2. Centered floating Obsidian card.
        let card = card_geometry(cw, ch, state.mode);
        draw_round_rect(
            hdc,
            &card,
            CORNER_RADIUS,
            BG_OBSIDIAN,
            Some(BORDER_ELEVATED),
        );

        // 3. Header brand voice: uppercase SCREENTIME.
        draw_tracked_caps(
            hdc,
            card.x + PAD,
            card.y + 20,
            "SCREENTIME",
            &fonts.brand,
            TEXT_MUTED,
            2,
        );

        // 4. Header badge: LIMIT REACHED in rounded rose badge.
        draw_block_badge(
            hdc,
            card.x + card.w - PAD,
            card.y + 16,
            "LIMIT REACHED",
            &fonts,
        );

        // 5. Header divider line.
        fill(
            hdc,
            &RECT {
                left: card.x + PAD,
                top: card.y + 48,
                right: card.x + card.w - PAD,
                bottom: card.y + 49,
            },
            BORDER_ELEVATED,
        );

        // 6. Headline: "Time's up for today"
        draw_text(
            hdc,
            card.x + PAD,
            card.y + 60,
            "Time's up for today",
            &fonts.display,
            TEXT_WHITE,
        );

        // 7. Prominent blocked app label in vibrant indigo.
        let label = ellipsize(hdc, &state.label, &fonts.app_label, card.w - PAD * 2);
        draw_text(
            hdc,
            card.x + PAD,
            card.y + 92,
            &label,
            &fonts.app_label,
            ACCENT_INDIGO_BRIGHT,
        );

        // 8. Clean sub-caption explaining the reason.
        let subcaption = match state.mode {
            OverlayMode::Buttons => {
                "This application reached its daily limit or scheduled downtime."
            }
            OverlayMode::PinExtend => "Enter master PIN to add 15 minutes, or quit the app below.",
        };
        draw_text(
            hdc,
            card.x + PAD,
            card.y + 118,
            subcaption,
            &fonts.caption,
            TEXT_MUTED,
        );

        // 9. Body actions based on mode.
        match state.mode {
            OverlayMode::Buttons => {
                let (quit, extend) = layout_buttons(&card);
                draw_button(
                    hdc,
                    &quit,
                    "QUIT APP",
                    BtnStyle::Secondary,
                    &fonts.btn,
                    TEXT_WHITE,
                );
                draw_button(
                    hdc,
                    &extend,
                    "+15 MIN EXTEND",
                    BtnStyle::Primary,
                    &fonts.btn,
                    TEXT_WHITE,
                );
            }
            OverlayMode::PinExtend => {
                let (gx, gy) = pad_origin(&card);

                // Section header: "ENTER MASTER PIN"
                draw_tracked_caps(
                    hdc,
                    gx,
                    gy - BTN_GAP - 52,
                    "ENTER MASTER PIN",
                    &fonts.pin_label,
                    TEXT_MUTED,
                    2,
                );

                // Masked entry: glowing dots (● ● ● ●) that light up indigo for each digit entered.
                let num_dots = state.pin.chars().count().clamp(4, PIN_MAX_DIGITS);
                let dot_spacing = 20;
                let dots_total_w = (num_dots as i32) * dot_spacing;
                let dots_start_x = card.x + (card.w - dots_total_w) / 2 + dot_spacing / 2;
                let dots_y = gy - BTN_GAP - 32;

                for i in 0..num_dots {
                    let dot_cx = dots_start_x + (i as i32) * dot_spacing;
                    if i < state.pin.chars().count() {
                        // Lit dot: glow outer ring + solid indigo core.
                        draw_circle(
                            hdc,
                            dot_cx,
                            dots_y,
                            6,
                            rgb(0x28, 0x2D, 0x54),
                            Some(ACCENT_INDIGO_BRIGHT),
                        );
                        draw_circle(hdc, dot_cx, dots_y, 3, ACCENT_INDIGO, Some(ACCENT_INDIGO));
                    } else {
                        // Unlit slot: dark background + subtle border.
                        draw_circle(
                            hdc,
                            dot_cx,
                            dots_y,
                            5,
                            BG_CARD_ELEVATED,
                            Some(BORDER_ELEVATED),
                        );
                        draw_circle(hdc, dot_cx, dots_y, 2, BORDER_ELEVATED, None);
                    }
                }

                // Incorrect PIN message in clear rose text.
                if state.wrong_pin {
                    draw_text_centered(
                        hdc,
                        card.x + card.w / 2,
                        gy - BTN_GAP - 16,
                        "Incorrect PIN. Please try again.",
                        &fonts.caption,
                        ACCENT_ROSE,
                    );
                }

                // 3x4 grid of tactile rounded keypad buttons.
                for (i, cell) in layout_pad(&card).iter().enumerate() {
                    match PAD_KEYS[i] {
                        "OK" => {
                            draw_button(
                                hdc,
                                cell,
                                "+15 MIN",
                                BtnStyle::Primary,
                                &fonts.btn,
                                TEXT_WHITE,
                            );
                        }
                        "C" => {
                            draw_button(
                                hdc,
                                cell,
                                "CLEAR",
                                BtnStyle::Secondary,
                                &fonts.btn,
                                TEXT_MUTED,
                            );
                        }
                        digit => {
                            draw_button(
                                hdc,
                                cell,
                                digit,
                                BtnStyle::Secondary,
                                &fonts.pin_digit,
                                TEXT_WHITE,
                            );
                        }
                    }
                }

                // Dedicated QUIT APP button below keypad
                let quit_btn = layout_pin_quit(&card);
                draw_button(
                    hdc,
                    &quit_btn,
                    "QUIT APP",
                    BtnStyle::Secondary,
                    &fonts.btn,
                    TEXT_WHITE,
                );
            }
        }
    }

    let _ = windows::Win32::Graphics::Gdi::EndPaint(hwnd, &ps);
}

/// Timer id for the fade-in ramp.
const FADE_TIMER_ID: usize = 1;
/// Ramp pacing: ~200 ms total (10 steps × 20 ms).
const FADE_STEP_MS: u32 = 20;
const FADE_ALPHA_STEP: u8 = 25;
/// Target layered window alpha for glass-like backdrop translucency.
const TARGET_ALPHA: u8 = 240;

/// Run one overlay to completion. Blocks the calling thread in a message pump
/// until the window is destroyed; call via [`spawn_overlay`] only.
fn run_overlay(
    rect: (i32, i32, i32, i32),
    callbacks: OverlayCallbacks,
    mode: OverlayMode,
    label: String,
    control: Arc<OverlayControl>,
) {
    unsafe {
        let hinstance = GetModuleHandleW(None).expect("module handle");
        let class_name = PCWSTR(w!("ScreentimeBlockOverlay").as_ptr());

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: class_name,
            ..Default::default()
        };
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

        // WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE covers the full app rect.
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

        // Start transparent and ramp to TARGET_ALPHA via WM_TIMER.
        let _ = SetLayeredWindowAttributes(
            hwnd,
            COLORREF(0),
            0,
            LAYERED_WINDOW_ATTRIBUTES_FLAGS(0x2), // LWA_ALPHA
        );

        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0).ok();
        if hook.is_none() {
            tracing::warn!("keyboard hook unavailable; overlay shows without input blocking");
        }

        SetTimer(hwnd, FADE_TIMER_ID, FADE_STEP_MS, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = KillTimer(hwnd, FADE_TIMER_ID);
        if let Some(hook) = hook {
            let _ = UnhookWindowsHookEx(hook);
        }
    }
}

/// Swallow all keys while the overlay is up, so the blocked app cannot be
/// driven by keyboard.
unsafe extern "system" fn keyboard_proc(_code: i32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
    LRESULT(1)
}
