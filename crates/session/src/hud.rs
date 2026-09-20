use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, Ellipse,
    EndPaint, InvalidateRect, RoundRect, SelectObject, SetBkMode, SetTextColor, DT_LEFT,
    DT_SINGLELINE, DT_VCENTER, HGDIOBJ, PS_SOLID, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW,
    GetWindowLongPtrW, GetWindowRect, IsIconic, IsWindow, IsWindowVisible, KillTimer, LoadCursorW,
    PostMessageW, PostQuitMessage, RegisterClassExW, SetCursor, SetLayeredWindowAttributes,
    SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, GWLP_USERDATA,
    GWL_EXSTYLE, HTCLIENT, IDC_SIZEALL, LWA_ALPHA, MSG, SWP_NOACTIVATE, SWP_NOOWNERZORDER,
    SWP_NOSENDCHANGING, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, WINDOW_EX_STYLE,
    WM_CLOSE, WM_DESTROY, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_NCHITTEST,
    WM_PAINT, WM_SETCURSOR, WM_TIMER, WM_USER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const WM_UPDATE_HUD: u32 = WM_USER + 1;
const WM_ANIM_TICK: u32 = WM_USER + 2;
const TRACK_TIMER_ID: usize = 1;
const TRACK_STEP_MS: u32 = 100; // 10 Hz position tracking
const ANIM_DURATION_MS: f32 = 240.0; // 240 ms Windows volume flyout curve

pub const HUD_W: i32 = 92;
pub const HUD_H: i32 = 28;

/// Color tokens for GDI fallback. Packed as `0x00_bb_gg_rr`.
const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}

// Midnight Cobalt / Slate Charcoal Dark tokens
const BG_DARK: COLORREF = rgb(0x0B, 0x0E, 0x17);
const BORDER_DARK: COLORREF = rgb(0x1E, 0x26, 0x38);
const TEXT_DARK: COLORREF = rgb(0xF8, 0xFA, 0xFC);

// Clean Titanium / Nordic Frost Light tokens
const BG_LIGHT: COLORREF = rgb(0xF8, 0xFA, 0xFC);
const BORDER_LIGHT: COLORREF = rgb(0xE2, 0xE8, 0xF0);
const TEXT_LIGHT: COLORREF = rgb(0x0F, 0x17, 0x2A);

// Indicator dot states
const DOT_COBALT: COLORREF = rgb(0x4F, 0x46, 0xE5); // Electric Cobalt
const DOT_AMBER: COLORREF = rgb(0xF5, 0x9E, 0x0B);
const DOT_CORAL: COLORREF = rgb(0xF4, 0x3F, 0x5E);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HudPosConfig {
    pub offset_x: i32,
    pub offset_y: i32,
    #[serde(default)]
    pub from_right: bool,
    #[serde(default)]
    pub from_bottom: bool,
}

fn hud_config_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join("screentime").join("hud_pos.json"))
}

fn load_hud_config() -> Option<HudPosConfig> {
    let path = hud_config_path()?;
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_hud_config(cfg: &HudPosConfig) {
    if let Some(path) = hud_config_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(cfg) {
            let _ = std::fs::write(path, data);
        }
    }
}

fn is_light_theme() -> bool {
    let path = std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join("screentime").join("theme.txt"));
    if let Some(p) = path {
        if let Ok(content) = std::fs::read_to_string(p) {
            let trimmed = content.trim();
            return trimmed.contains("clean-titanium")
                || trimmed.contains("nordic-frost")
                || trimmed.contains("light")
                || trimmed.contains("titanium")
                || trimmed.contains("frost");
        }
    }
    false
}

pub const PHASE_ENTERING: u8 = 0;
pub const PHASE_SETTLED: u8 = 1;
pub const PHASE_EXITING: u8 = 2;
pub const PHASE_CLOSED: u8 = 3;

pub struct HudAnimShared {
    pub phase: std::sync::atomic::AtomicU8,
    pub cancel: AtomicBool,
    pub start_y: std::sync::atomic::AtomicI32,
    pub target_y: std::sync::atomic::AtomicI32,
    pub current_y: std::sync::atomic::AtomicI32,
    pub final_y: std::sync::atomic::AtomicI32,
    pub anim_start_y: std::sync::atomic::AtomicI32,
}

impl HudAnimShared {
    pub fn new(final_y: i32, anim_start_y: i32) -> Self {
        Self {
            phase: std::sync::atomic::AtomicU8::new(PHASE_ENTERING),
            cancel: AtomicBool::new(false),
            start_y: std::sync::atomic::AtomicI32::new(anim_start_y),
            target_y: std::sync::atomic::AtomicI32::new(final_y),
            current_y: std::sync::atomic::AtomicI32::new(anim_start_y),
            final_y: std::sync::atomic::AtomicI32::new(final_y),
            anim_start_y: std::sync::atomic::AtomicI32::new(anim_start_y),
        }
    }

    pub fn trigger_exit(&self) {
        let cur = self.phase.load(Ordering::Relaxed);
        if cur == PHASE_EXITING || cur == PHASE_CLOSED {
            return;
        }
        let cur_y = self.current_y.load(Ordering::Relaxed);
        self.start_y.store(cur_y, Ordering::Relaxed);
        self.target_y
            .store(self.anim_start_y.load(Ordering::Relaxed), Ordering::Relaxed);
        self.phase.store(PHASE_EXITING, Ordering::Relaxed);
    }

    pub fn trigger_enter(&self) {
        let cur = self.phase.load(Ordering::Relaxed);
        if cur == PHASE_ENTERING || cur == PHASE_SETTLED {
            return;
        }
        let cur_y = self.current_y.load(Ordering::Relaxed);
        self.start_y.store(cur_y, Ordering::Relaxed);
        self.target_y
            .store(self.final_y.load(Ordering::Relaxed), Ordering::Relaxed);
        self.phase.store(PHASE_ENTERING, Ordering::Relaxed);
    }
}

fn compute_target_pos(wr: &RECT, cfg: Option<HudPosConfig>) -> (i32, i32) {
    let tw = (wr.right - wr.left).max(1);
    let _th = (wr.bottom - wr.top).max(1);

    if let Some(c) = cfg {
        let raw_x = if c.from_right {
            wr.right - c.offset_x - HUD_W
        } else {
            wr.left + c.offset_x
        };
        let raw_y = if c.from_bottom {
            wr.bottom - c.offset_y - HUD_H
        } else {
            wr.top + c.offset_y
        };
        let x = raw_x.clamp(wr.left + 4, (wr.right - HUD_W - 4).max(wr.left + 4));
        let y = raw_y.clamp(wr.top + 4, (wr.bottom - HUD_H - 4).max(wr.top + 4));
        (x, y)
    } else {
        // Default: centered horizontally at top of target window
        let target_cx = wr.left + tw / 2;
        let x = (target_cx - HUD_W / 2).clamp(wr.left + 8, (wr.right - HUD_W - 8).max(wr.left + 8));
        let y = (wr.top + 10).clamp(wr.top + 4, (wr.bottom - HUD_H - 4).max(wr.top + 4));
        (x, y)
    }
}

struct HudState {
    target_hwnd: isize,
    remaining_secs: i64,
    is_timer: bool,
    is_light: bool,
    last_x: i32,
    last_y: i32,
    is_hidden: bool,
    is_dragging: bool,
    drag_start_cursor: POINT,
    drag_start_win: (i32, i32),
    anim_active: bool,
    anim_ctrl: Arc<HudAnimShared>,
    pos_config: Option<HudPosConfig>,
    mpo: Option<crate::mpo::MpoHudRenderer>,
}

pub struct HudOverlayRun {
    thread: Option<std::thread::JoinHandle<()>>,
    hwnd: HWND,
    controller: Arc<HudAnimShared>,
}

impl HudOverlayRun {
    pub fn is_exiting(&self) -> bool {
        self.controller.phase.load(Ordering::Relaxed) == PHASE_EXITING
    }

    pub fn is_alive(&self) -> bool {
        let p = self.controller.phase.load(Ordering::Relaxed);
        p != PHASE_CLOSED && !self.hwnd.0.is_null() && unsafe { IsWindow(self.hwnd).as_bool() }
    }

    pub fn start_exit(&self) {
        self.controller.trigger_exit();
    }

    pub fn reverse_to_enter(&self) {
        self.controller.trigger_enter();
    }

    pub fn dismiss(mut self) {
        self.controller.cancel.store(true, Ordering::Relaxed);
        if !self.hwnd.0.is_null() {
            unsafe {
                let _ = PostMessageW(self.hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    pub fn update(&self, remaining_secs: i64, is_timer: bool) {
        if !self.hwnd.0.is_null() {
            unsafe {
                let _ = PostMessageW(
                    self.hwnd,
                    WM_UPDATE_HUD,
                    WPARAM(remaining_secs as usize),
                    LPARAM(if is_timer { 1 } else { 0 }),
                );
            }
        }
    }
}

pub fn spawn_hud_overlay(target_hwnd: isize, target_rect: (i32, i32, i32, i32)) -> HudOverlayRun {
    let (tx, ty, tw, th) = target_rect;
    let wr = RECT {
        left: tx,
        top: ty,
        right: tx + tw,
        bottom: ty + th,
    };
    let saved_cfg = load_hud_config();
    let (final_x, final_y) = compute_target_pos(&wr, saved_cfg);

    // Slide down from top if in top half; slide up from bottom if in bottom half
    let is_bottom_half = (final_y - ty) > (th.max(1) / 2);
    let anim_start_y = if is_bottom_half {
        final_y + 24
    } else {
        final_y - 24
    };

    let anim_shared = Arc::new(HudAnimShared::new(final_y, anim_start_y));
    let anim_ctrl_for_thread = anim_shared.clone();
    let anim_ctrl_for_state = anim_shared.clone();

    let (sender, receiver) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("ScreentimeHudClass");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            hCursor: LoadCursorW(None, IDC_SIZEALL).unwrap_or_default(),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);

        let is_light = is_light_theme();

        let state = Box::into_raw(Box::new(HudState {
            target_hwnd,
            remaining_secs: 0,
            is_timer: false,
            is_light,
            last_x: final_x,
            last_y: anim_start_y,
            is_hidden: false,
            is_dragging: false,
            drag_start_cursor: POINT::default(),
            drag_start_win: (final_x, anim_start_y),
            anim_active: true,
            anim_ctrl: anim_ctrl_for_state,
            pos_config: saved_cfg,
            mpo: None,
        }));

        // WS_EX_NOACTIVATE ensures dragging/clicking never steals keyboard focus from the active app.
        // WS_EX_TRANSPARENT is intentionally omitted so the HUD can receive direct drag events.
        let mpo_style =
            WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WINDOW_EX_STYLE(0x00200000);

        let Ok(hwnd) = CreateWindowExW(
            mpo_style,
            class_name,
            PCWSTR::null(),
            WS_POPUP,
            final_x,
            anim_start_y,
            HUD_W,
            HUD_H,
            HWND(std::ptr::null_mut()),
            None,
            instance,
            Some(state as *const c_void),
        ) else {
            drop(Box::from_raw(state));
            return;
        };

        // Try initializing hardware MPO DirectComposition renderer; fallback to GDI layered window if unavailable or disabled.
        let is_gdi_forced = std::env::var_os("SCREENTIME_HUD_GDI").is_some();
        let mut mpo_renderer = None;
        if !is_gdi_forced {
            match crate::mpo::MpoHudRenderer::new(hwnd) {
                Ok(mut mpo) => {
                    let _ = mpo.render_frame(0, false, is_light);
                    mpo_renderer = Some(mpo);
                    tracing::debug!("Hardware MPO overlay initialized successfully for HUD");
                }
                Err(e) => {
                    tracing::warn!(
                        "MPO initialization failed ({:#}), falling back to GDI layered window",
                        e
                    );
                }
            }
        }
        if mpo_renderer.is_none() {
            let gdi_style = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
            let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, gdi_style.0 as isize);
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
        }
        (*state).mpo = mpo_renderer;

        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetTimer(hwnd, TRACK_TIMER_ID, TRACK_STEP_MS, None);

        // Dedicated 120 FPS high-precision animation worker thread
        let anim_hwnd = hwnd.0 as isize;
        let anim_ctrl = anim_ctrl_for_thread;
        std::thread::spawn(move || {
            #[link(name = "winmm")]
            extern "system" {
                fn timeBeginPeriod(uPeriod: u32) -> u32;
                fn timeEndPeriod(uPeriod: u32) -> u32;
            }

            let _ = timeBeginPeriod(1);
            let hwnd = HWND(anim_hwnd as *mut c_void);

            let mut active_phase = PHASE_ENTERING;
            let mut anim_start = Instant::now();
            let mut from_y = anim_ctrl.start_y.load(Ordering::Relaxed);
            let mut to_y = anim_ctrl.target_y.load(Ordering::Relaxed);

            while !anim_ctrl.cancel.load(Ordering::Relaxed) {
                let current_phase = anim_ctrl.phase.load(Ordering::Relaxed);
                if current_phase == PHASE_CLOSED {
                    break;
                }

                // Detect phase transition (e.g. exit started, or reversed back to entrance)
                if current_phase != active_phase {
                    active_phase = current_phase;
                    anim_start = Instant::now();
                    from_y = anim_ctrl.start_y.load(Ordering::Relaxed);
                    to_y = anim_ctrl.target_y.load(Ordering::Relaxed);
                }

                if active_phase == PHASE_ENTERING {
                    let elapsed = anim_start.elapsed().as_secs_f32() * 1000.0;
                    let t = (elapsed / ANIM_DURATION_MS).clamp(0.0, 1.0);
                    // Windows 11 cubic ease-out
                    let ease = 1.0 - (1.0 - t).powi(3);
                    let cur_y = (from_y as f32 + (to_y - from_y) as f32 * ease).round() as i32;
                    anim_ctrl.current_y.store(cur_y, Ordering::Relaxed);

                    if !IsWindow(hwnd).as_bool() {
                        break;
                    }
                    let _ = PostMessageW(
                        hwnd,
                        WM_ANIM_TICK,
                        WPARAM(cur_y as usize),
                        LPARAM(if t >= 1.0 { 1 } else { 0 }),
                    );

                    if t >= 1.0 {
                        anim_ctrl.phase.store(PHASE_SETTLED, Ordering::Relaxed);
                        active_phase = PHASE_SETTLED;
                    }
                } else if active_phase == PHASE_EXITING {
                    let elapsed = anim_start.elapsed().as_secs_f32() * 1000.0;
                    let t = (elapsed / ANIM_DURATION_MS).clamp(0.0, 1.0);
                    // Smooth cubic ease-in: sliding back off-screen in the direction it entered
                    let ease = t.powi(3);
                    let cur_y = (from_y as f32 + (to_y - from_y) as f32 * ease).round() as i32;
                    anim_ctrl.current_y.store(cur_y, Ordering::Relaxed);

                    if !IsWindow(hwnd).as_bool() {
                        break;
                    }
                    let _ = PostMessageW(
                        hwnd,
                        WM_ANIM_TICK,
                        WPARAM(cur_y as usize),
                        LPARAM(if t >= 1.0 { 1 } else { 0 }),
                    );

                    if t >= 1.0 {
                        anim_ctrl.phase.store(PHASE_CLOSED, Ordering::Relaxed);
                        let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                        break;
                    }
                }

                std::thread::sleep(Duration::from_millis(8)); // ~120 Hz tick
            }

            let _ = timeEndPeriod(1);
        });

        sender.send(hwnd.0 as isize).unwrap();

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = KillTimer(hwnd, TRACK_TIMER_ID);
    });

    let hwnd = HWND(receiver.recv().unwrap() as *mut c_void);
    HudOverlayRun {
        thread: Some(thread),
        hwnd,
        controller: anim_shared,
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW;
            let state = (*cs).lpCreateParams as *const HudState;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_NCHITTEST => {
            // Return HTCLIENT so window receives mouse clicks for dragging,
            // while WS_EX_NOACTIVATE prevents focus stealing.
            LRESULT(HTCLIENT as isize)
        }
        WM_SETCURSOR => {
            // Display 4-way move cursor to indicate draggable HUD
            let _ = SetCursor(LoadCursorW(None, IDC_SIZEALL).unwrap_or_default());
            LRESULT(1)
        }
        WM_LBUTTONDOWN => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut HudState;
            if !state.is_null() {
                let st = &mut *state;
                st.anim_ctrl.cancel.store(true, Ordering::Relaxed);
                st.anim_ctrl.phase.store(PHASE_SETTLED, Ordering::Relaxed);
                st.anim_active = false;
                st.is_dragging = true;
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                st.drag_start_cursor = pt;
                st.drag_start_win = (st.last_x, st.last_y);
                SetCapture(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut HudState;
            if !state.is_null() {
                let st = &mut *state;
                if st.is_dragging {
                    let mut pt = POINT::default();
                    let _ = GetCursorPos(&mut pt);
                    let dx = pt.x - st.drag_start_cursor.x;
                    let dy = pt.y - st.drag_start_cursor.y;
                    let mut new_x = st.drag_start_win.0 + dx;
                    let mut new_y = st.drag_start_win.1 + dy;

                    let target = HWND(st.target_hwnd as *mut c_void);
                    if !target.0.is_null() && IsWindow(target).as_bool() {
                        let mut wr = RECT::default();
                        if GetWindowRect(target, &mut wr).is_ok() {
                            new_x =
                                new_x.clamp(wr.left + 4, (wr.right - HUD_W - 4).max(wr.left + 4));
                            new_y =
                                new_y.clamp(wr.top + 4, (wr.bottom - HUD_H - 4).max(wr.top + 4));
                        }
                    }

                    if new_x != st.last_x || new_y != st.last_y {
                        st.last_x = new_x;
                        st.last_y = new_y;
                        let _ = SetWindowPos(
                            hwnd,
                            HWND(std::ptr::null_mut()),
                            new_x,
                            new_y,
                            HUD_W,
                            HUD_H,
                            SWP_NOACTIVATE
                                | SWP_NOOWNERZORDER
                                | SWP_NOSIZE
                                | SWP_NOZORDER
                                | SWP_NOSENDCHANGING,
                        );
                    }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut HudState;
            if !state.is_null() {
                let st = &mut *state;
                if st.is_dragging {
                    st.is_dragging = false;
                    let _ = ReleaseCapture();

                    let target = HWND(st.target_hwnd as *mut c_void);
                    if !target.0.is_null() && IsWindow(target).as_bool() {
                        let mut wr = RECT::default();
                        if GetWindowRect(target, &mut wr).is_ok() {
                            let tw = (wr.right - wr.left).max(1);
                            let th = (wr.bottom - wr.top).max(1);
                            let rel_x = st.last_x - wr.left;
                            let rel_y = st.last_y - wr.top;
                            let from_right = rel_x > tw / 2;
                            let from_bottom = rel_y > th / 2;
                            let offset_x = if from_right {
                                wr.right - (st.last_x + HUD_W)
                            } else {
                                rel_x
                            };
                            let offset_y = if from_bottom {
                                wr.bottom - (st.last_y + HUD_H)
                            } else {
                                rel_y
                            };

                            let cfg = HudPosConfig {
                                offset_x,
                                offset_y,
                                from_right,
                                from_bottom,
                            };
                            st.pos_config = Some(cfg);
                            save_hud_config(&cfg);
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_UPDATE_HUD => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut HudState;
            if !state.is_null() {
                let st = &mut *state;
                st.remaining_secs = wparam.0 as i64;
                st.is_timer = lparam.0 != 0;
                st.is_light = is_light_theme();
                if let Some(mpo) = &mut st.mpo {
                    let _ = mpo.render_frame(st.remaining_secs, st.is_timer, st.is_light);
                } else {
                    let _ = InvalidateRect(hwnd, None, true);
                }
            }
            LRESULT(0)
        }
        WM_ANIM_TICK => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut HudState;
            if !state.is_null() {
                let st = &mut *state;
                if st.anim_active && !st.is_dragging {
                    let cur_y = wparam.0 as i32;
                    let is_finished = lparam.0 != 0;
                    st.last_y = cur_y;
                    let _ = SetWindowPos(
                        hwnd,
                        HWND(std::ptr::null_mut()),
                        st.last_x,
                        cur_y,
                        HUD_W,
                        HUD_H,
                        SWP_NOACTIVATE
                            | SWP_NOOWNERZORDER
                            | SWP_NOSIZE
                            | SWP_NOZORDER
                            | SWP_NOSENDCHANGING,
                    );
                    if is_finished {
                        st.anim_active = false;
                    }
                }
            }
            LRESULT(0)
        }
        WM_TIMER => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut HudState;
            if state.is_null() {
                return LRESULT(0);
            }
            let st = &mut *state;

            if wparam.0 == TRACK_TIMER_ID {
                if st.is_dragging {
                    return LRESULT(0);
                }

                let target = HWND(st.target_hwnd as *mut c_void);
                if !target.0.is_null() && IsWindow(target).as_bool() {
                    if IsIconic(target).as_bool() {
                        if !st.is_hidden {
                            st.is_hidden = true;
                            let _ = ShowWindow(hwnd, SW_HIDE);
                        }
                    } else if IsWindowVisible(target).as_bool() {
                        if st.is_hidden {
                            st.is_hidden = false;
                            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                        }
                        let mut wr = RECT::default();
                        if GetWindowRect(target, &mut wr).is_ok() {
                            let tw = wr.right - wr.left;
                            let th = wr.bottom - wr.top;
                            if tw > 0 && th > 0 {
                                let (target_x, target_y) = compute_target_pos(&wr, st.pos_config);
                                st.anim_ctrl.final_y.store(target_y, Ordering::Relaxed);
                                let is_bottom_half = (target_y - wr.top) > (th.max(1) / 2);
                                let offscreen_y = if is_bottom_half {
                                    target_y + 24
                                } else {
                                    target_y - 24
                                };
                                st.anim_ctrl
                                    .anim_start_y
                                    .store(offscreen_y, Ordering::Relaxed);

                                if st.anim_ctrl.phase.load(Ordering::Relaxed) == PHASE_SETTLED {
                                    if target_x != st.last_x || target_y != st.last_y {
                                        st.last_x = target_x;
                                        st.last_y = target_y;
                                        st.anim_ctrl.current_y.store(target_y, Ordering::Relaxed);
                                        let _ = SetWindowPos(
                                            hwnd,
                                            HWND(std::ptr::null_mut()),
                                            target_x,
                                            target_y,
                                            HUD_W,
                                            HUD_H,
                                            SWP_NOACTIVATE
                                                | SWP_NOOWNERZORDER
                                                | SWP_NOSIZE
                                                | SWP_NOZORDER
                                                | SWP_NOSENDCHANGING,
                                        );
                                    }
                                } else if target_x != st.last_x {
                                    st.last_x = target_x;
                                    let _ = SetWindowPos(
                                        hwnd,
                                        HWND(std::ptr::null_mut()),
                                        target_x,
                                        st.last_y,
                                        HUD_W,
                                        HUD_H,
                                        SWP_NOACTIVATE
                                            | SWP_NOOWNERZORDER
                                            | SWP_NOSIZE
                                            | SWP_NOZORDER
                                            | SWP_NOSENDCHANGING,
                                    );
                                }
                            }
                        }
                    }
                } else {
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut HudState;
            if state.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let st = &mut *state;

            if let Some(mpo) = &mut st.mpo {
                let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut ps);
                let _ = mpo.render_frame(st.remaining_secs, st.is_timer, st.is_light);
                let _ = EndPaint(hwnd, &ps);
                return LRESULT(0);
            }

            let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rect = RECT::default();
            windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect).unwrap();

            let bg_color = if st.is_light { BG_LIGHT } else { BG_DARK };
            let border_color = if st.is_light {
                BORDER_LIGHT
            } else {
                BORDER_DARK
            };
            let text_color = if st.is_light { TEXT_LIGHT } else { TEXT_DARK };
            let dot_color = if st.remaining_secs <= 60 {
                DOT_CORAL
            } else if st.is_timer {
                DOT_AMBER
            } else {
                DOT_COBALT
            };

            let hbrush = CreateSolidBrush(bg_color);
            let hpen = CreatePen(PS_SOLID, 1, border_color);

            let old_brush = SelectObject(hdc, HGDIOBJ(hbrush.0));
            let old_pen = SelectObject(hdc, HGDIOBJ(hpen.0));

            // Solid rounded pill
            let _ = RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, 14, 14);

            // Glowing indicator dot
            let dot_brush = CreateSolidBrush(dot_color);
            let dot_pen = CreatePen(PS_SOLID, 1, dot_color);
            let prev_b = SelectObject(hdc, HGDIOBJ(dot_brush.0));
            let prev_p = SelectObject(hdc, HGDIOBJ(dot_pen.0));
            let dot_cx = 14;
            let dot_cy = (rect.bottom - rect.top) / 2;
            let dot_r = 3;
            let _ = Ellipse(
                hdc,
                dot_cx - dot_r,
                dot_cy - dot_r,
                dot_cx + dot_r + 1,
                dot_cy + dot_r + 1,
            );
            SelectObject(hdc, prev_b);
            SelectObject(hdc, prev_p);
            let _ = DeleteObject(HGDIOBJ(dot_brush.0));
            let _ = DeleteObject(HGDIOBJ(dot_pen.0));

            // Monospace tabular countdown digits
            let font = CreateFontW(
                -13,
                0,
                0,
                0,
                700,
                0,
                0,
                0,
                1, // DEFAULT_CHARSET
                0,
                0,
                5, // CLEARTYPE_QUALITY
                0,
                w!("Consolas"),
            );
            let old_font = SelectObject(hdc, HGDIOBJ(font.0));
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, text_color);

            let hrs = st.remaining_secs / 3600;
            let mins = (st.remaining_secs % 3600) / 60;
            let secs = st.remaining_secs % 60;
            let text = if hrs > 0 {
                format!("{}:{:02}:{:02}", hrs, mins, secs)
            } else {
                format!("{:02}:{:02}", mins, secs)
            };
            let mut wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();

            let mut text_rect = RECT {
                left: 23,
                top: 0,
                right: rect.right - 6,
                bottom: rect.bottom,
            };

            DrawTextW(
                hdc,
                &mut wide,
                &mut text_rect,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );

            SelectObject(hdc, old_brush);
            SelectObject(hdc, old_pen);
            SelectObject(hdc, old_font);
            let _ = DeleteObject(HGDIOBJ(hbrush.0));
            let _ = DeleteObject(HGDIOBJ(hpen.0));
            let _ = DeleteObject(HGDIOBJ(font.0));

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut HudState;
            if !state.is_null() {
                let st = &mut *state;
                st.anim_ctrl.cancel.store(true, Ordering::Relaxed);
                st.anim_ctrl.phase.store(PHASE_CLOSED, Ordering::Relaxed);
                st.anim_active = false;
                drop(Box::from_raw(state));
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
