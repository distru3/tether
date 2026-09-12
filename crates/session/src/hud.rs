use std::ffi::c_void;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, Ellipse,
    EndPaint, InvalidateRect, RoundRect, SelectObject, SetBkMode, SetTextColor, DT_LEFT,
    DT_SINGLELINE, DT_VCENTER, HGDIOBJ, PS_SOLID, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, GetWindowRect, IsIconic, IsWindow, IsWindowVisible, KillTimer, PostMessageW,
    PostQuitMessage, RegisterClassExW, SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, TranslateMessage, GWLP_USERDATA, HWND_TOPMOST, LWA_ALPHA, MSG,
    SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE,
    WM_CLOSE, WM_DESTROY, WM_NCCREATE, WM_PAINT, WM_TIMER, WM_USER, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
};

const WM_UPDATE_HUD: u32 = WM_USER + 1;
const TRACK_TIMER_ID: usize = 1;
const TRACK_STEP_MS: u32 = 16; // ~60 FPS real-time clamping

pub const HUD_W: i32 = 92;
pub const HUD_H: i32 = 28;

/// Color Hunt palette tokens for GDI. Packed as `0x00_bb_gg_rr`.
const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}

const BG_OBSIDIAN_PURPLE: COLORREF = rgb(0x21, 0x0F, 0x37);
const BORDER_AMBER: COLORREF = rgb(0xDC, 0xA0, 0x6D);
const BORDER_TERRACOTTA: COLORREF = rgb(0xA5, 0x5B, 0x4B);
const TEXT_AMBER: COLORREF = rgb(0xDC, 0xA0, 0x6D);
const TEXT_IVORY: COLORREF = rgb(0xF5, 0xEE, 0xF8);

struct HudState {
    target_hwnd: isize,
    remaining_secs: i64,
    is_timer: bool,
    last_x: i32,
    last_y: i32,
}

pub struct HudOverlayRun {
    thread: Option<std::thread::JoinHandle<()>>,
    hwnd: HWND,
}

impl HudOverlayRun {
    pub fn dismiss(mut self) {
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
    let (tx, ty, tw, _th) = target_rect;
    let target_cx = tx + tw / 2;
    let init_x = (target_cx - HUD_W / 2).clamp(tx + 8, (tx + tw - HUD_W - 8).max(tx + 8));
    let init_y = ty + 10;

    let (sender, receiver) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("ScreentimeHudClass");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);

        let state = Box::into_raw(Box::new(HudState {
            target_hwnd,
            remaining_secs: 0,
            is_timer: false,
            last_x: init_x,
            last_y: init_y,
        }));

        let Ok(hwnd) = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
            class_name,
            PCWSTR::null(),
            WS_POPUP | WS_VISIBLE,
            init_x,
            init_y,
            HUD_W,
            HUD_H,
            None,
            None,
            instance,
            Some(state as *const c_void),
        ) else {
            drop(Box::from_raw(state));
            return;
        };

        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 235, LWA_ALPHA);
        let _ = SetTimer(hwnd, TRACK_TIMER_ID, TRACK_STEP_MS, None);

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
        WM_UPDATE_HUD => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut HudState;
            if !state.is_null() {
                let st = &mut *state;
                st.remaining_secs = wparam.0 as i64;
                st.is_timer = lparam.0 != 0;
                let _ = InvalidateRect(hwnd, None, true);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == TRACK_TIMER_ID {
                let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut HudState;
                if !state.is_null() {
                    let st = &mut *state;
                    let target = HWND(st.target_hwnd as *mut c_void);
                    if !target.0.is_null() && IsWindow(target).as_bool() {
                        if IsIconic(target).as_bool() {
                            let _ = ShowWindow(hwnd, SW_HIDE);
                        } else if IsWindowVisible(target).as_bool() {
                            let mut wr = RECT::default();
                            if GetWindowRect(target, &mut wr).is_ok() {
                                let tw = wr.right - wr.left;
                                if tw > 0 && wr.bottom > wr.top {
                                    let target_cx = wr.left + tw / 2;
                                    let new_x = (target_cx - HUD_W / 2).clamp(
                                        wr.left + 8,
                                        (wr.right - HUD_W - 8).max(wr.left + 8),
                                    );
                                    let new_y = wr.top + 10;
                                    if new_x != st.last_x || new_y != st.last_y {
                                        st.last_x = new_x;
                                        st.last_y = new_y;
                                        let _ = SetWindowPos(
                                            hwnd,
                                            HWND_TOPMOST,
                                            new_x,
                                            new_y,
                                            HUD_W,
                                            HUD_H,
                                            SWP_NOACTIVATE
                                                | SWP_NOOWNERZORDER
                                                | SWP_NOSIZE
                                                | SWP_SHOWWINDOW,
                                        );
                                    }
                                }
                            }
                            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                        }
                    } else {
                        // Target window closed or invalid
                        let _ = DestroyWindow(hwnd);
                    }
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const HudState;
            if state.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let st = &*state;

            let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rect = RECT::default();
            windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect).unwrap();

            let border_color = if st.is_timer {
                BORDER_AMBER
            } else {
                BORDER_TERRACOTTA
            };

            let hbrush = CreateSolidBrush(BG_OBSIDIAN_PURPLE);
            let hpen = CreatePen(PS_SOLID, 1, border_color);

            let old_brush = SelectObject(hdc, HGDIOBJ(hbrush.0));
            let old_pen = SelectObject(hdc, HGDIOBJ(hpen.0));

            // Smooth 14px rounded pill
            let _ = RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, 14, 14);

            // Left glowing indicator dot
            let dot_color = if st.is_timer {
                BORDER_AMBER
            } else {
                BORDER_TERRACOTTA
            };
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
            SetTextColor(hdc, if st.is_timer { TEXT_AMBER } else { TEXT_IVORY });

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
                drop(Box::from_raw(state));
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
