use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, DeleteObject, ExtTextOutW, FillRect, SelectObject, SetBkMode,
    SetTextColor, ETO_OPTIONS, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostMessageW,
    PostQuitMessage, RegisterClassExW, SetLayeredWindowAttributes, TranslateMessage, MSG, WM_CLOSE,
    WM_DESTROY, WM_PAINT, WM_USER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
};

const WM_UPDATE_HUD: u32 = WM_USER + 1;

pub struct HudOverlayRun {
    thread: Option<std::thread::JoinHandle<()>>,
    hwnd: HWND,
}

impl HudOverlayRun {
    pub fn dismiss(mut self) {
        if !self.hwnd.0.is_null() {
            unsafe { PostMessageW(self.hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap() };
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    pub fn update(&self, remaining_secs: i64, is_timer: bool) {
        if !self.hwnd.0.is_null() {
            unsafe {
                PostMessageW(
                    self.hwnd,
                    WM_UPDATE_HUD,
                    WPARAM(remaining_secs as usize),
                    LPARAM(if is_timer { 1 } else { 0 }),
                )
                .unwrap();
            }
        }
    }
}

pub fn spawn_hud_overlay(target_rect: (i32, i32, i32, i32)) -> HudOverlayRun {
    let (tx, rx) = std::sync::mpsc::channel();
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

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
            class_name,
            PCWSTR::null(),
            WS_POPUP | WS_VISIBLE,
            target_rect.0 + (target_rect.2 / 2) - 30,
            target_rect.1 + 10,
            60,
            24,
            None,
            None,
            instance,
            None,
        )
        .unwrap();

        SetLayeredWindowAttributes(
            hwnd,
            COLORREF(0),
            220,
            windows::Win32::UI::WindowsAndMessaging::LWA_ALPHA,
        )
        .unwrap();
        tx.send(hwnd.0 as isize).unwrap();

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });

    let hwnd = HWND(rx.recv().unwrap() as *mut std::ffi::c_void);
    HudOverlayRun {
        thread: Some(thread),
        hwnd,
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    static mut REMAINING: i64 = 0;
    static mut IS_TIMER: bool = false;

    match msg {
        WM_UPDATE_HUD => {
            REMAINING = wparam.0 as i64;
            IS_TIMER = lparam.0 != 0;
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, true);
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
            let hdc = windows::Win32::Graphics::Gdi::BeginPaint(hwnd, &mut ps);

            let bg_color = if IS_TIMER {
                COLORREF(0x00_00_8C_FF)
            } else {
                COLORREF(0x00_33_33_33)
            };
            let hbrush = CreateSolidBrush(bg_color);
            let mut rect = RECT::default();
            windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect).unwrap();
            FillRect(hdc, &rect, hbrush);
            let _ = DeleteObject(hbrush);

            let font = CreateFontW(16, 0, 0, 0, 600, 0, 0, 0, 0, 0, 0, 0, 0, w!("Segoe UI"));
            let old_font = SelectObject(hdc, font);
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, COLORREF(0x00_FF_FF_FF));

            let m = REMAINING / 60;
            let s = REMAINING % 60;
            let text = format!("{:02}:{:02}", m, s);
            let wide: Vec<u16> = text.encode_utf16().collect();
            let _ = ExtTextOutW(
                hdc,
                10,
                2,
                ETO_OPTIONS(0),
                None,
                PCWSTR::from_raw(wide.as_ptr()),
                wide.len() as u32,
                None,
            );

            SelectObject(hdc, old_font);
            let _ = DeleteObject(font);
            let _ = windows::Win32::Graphics::Gdi::EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
