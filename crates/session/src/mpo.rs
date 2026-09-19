//! Hardware Multiplane Overlay (MPO) timer renderer for Tether.
//!
//! Uses DirectX 11, DXGI flip-model swapchain (`DXGI_SWAP_EFFECT_FLIP_DISCARD`),
//! DirectComposition, and Direct2D. By using an independent hardware plane with
//! premultiplied alpha (`DXGI_ALPHA_MODE_PREMULTIPLIED`) and a window without
//! GDI redirection (`WS_EX_NOREDIRECTIONBITMAP`), Windows DWM can present the overlay
//! directly in hardware (Plane 1) without forcing full-screen games on Plane 0 to drop
//! from Hardware Independent Flip (iFlip / DirectFlip) into Composed Flip mode.
//!
//! This preserves:
//! - AMD Radeon Chill / FRTC driver limiters
//! - Variable Refresh Rate (AMD FreeSync / Nvidia G-Sync)
//! - Zero additional frame display latency

#![allow(unsafe_code)]

use anyhow::{Context, Result};
use std::mem::ManuallyDrop;
use windows::core::{w, Interface};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Device, ID2D1DeviceContext, ID2D1Factory1, ID2D1RenderTarget,
    ID2D1SolidColorBrush, D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET,
    D2D1_BITMAP_PROPERTIES1, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_10_1,
    D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_BOLD,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGIAdapter, IDXGIDevice, IDXGIFactory2, IDXGISwapChain1, DXGI_PRESENT, DXGI_SCALING_STRETCH,
    DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
};

pub const HUD_W: u32 = 92;
pub const HUD_H: u32 = 28;

// Modern color tokens for Direct2D (Midnight Cobalt, Cyber Emerald, Clean Titanium, Nordic Frost)
const COLOR_BG_DARK: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 11.0 / 255.0,
    g: 14.0 / 255.0,
    b: 23.0 / 255.0,
    a: 1.0,
};
const COLOR_BORDER_DARK: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 30.0 / 255.0,
    g: 38.0 / 255.0,
    b: 56.0 / 255.0,
    a: 1.0,
};
const COLOR_TEXT_DARK: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 248.0 / 255.0,
    g: 250.0 / 255.0,
    b: 252.0 / 255.0,
    a: 1.0,
};

const COLOR_BG_LIGHT: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 248.0 / 255.0,
    g: 250.0 / 255.0,
    b: 252.0 / 255.0,
    a: 1.0,
};
const COLOR_BORDER_LIGHT: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 226.0 / 255.0,
    g: 232.0 / 255.0,
    b: 240.0 / 255.0,
    a: 1.0,
};
const COLOR_TEXT_LIGHT: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 15.0 / 255.0,
    g: 23.0 / 255.0,
    b: 42.0 / 255.0,
    a: 1.0,
};

const COLOR_COBALT: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 79.0 / 255.0,
    g: 70.0 / 255.0,
    b: 229.0 / 255.0,
    a: 1.0,
};
const COLOR_AMBER: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 245.0 / 255.0,
    g: 158.0 / 255.0,
    b: 11.0 / 255.0,
    a: 1.0,
};
const COLOR_CORAL: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 244.0 / 255.0,
    g: 63.0 / 255.0,
    b: 94.0 / 255.0,
    a: 1.0,
};

/// Hardware Multiplane Overlay renderer instance attached to an HWND.
pub struct MpoHudRenderer {
    _d3d11_device: ID3D11Device,
    _dcomp_device: IDCompositionDevice,
    _dcomp_target: IDCompositionTarget,
    _dcomp_visual: IDCompositionVisual,
    swap_chain: IDXGISwapChain1,
    d2d_context: ID2D1DeviceContext,
    render_target: ID2D1RenderTarget,
    text_format: IDWriteTextFormat,
    brush_bg_dark: ID2D1SolidColorBrush,
    brush_border_dark: ID2D1SolidColorBrush,
    brush_text_dark: ID2D1SolidColorBrush,
    brush_bg_light: ID2D1SolidColorBrush,
    brush_border_light: ID2D1SolidColorBrush,
    brush_text_light: ID2D1SolidColorBrush,
    brush_cobalt: ID2D1SolidColorBrush,
    brush_amber: ID2D1SolidColorBrush,
    brush_coral: ID2D1SolidColorBrush,
}

impl MpoHudRenderer {
    /// Creates a hardware MPO renderer for the specified HWND.
    ///
    /// Requires the HWND to be created with `WS_POPUP` and preferably `WS_EX_NOREDIRECTIONBITMAP`.
    pub fn new(hwnd: HWND) -> Result<Self> {
        unsafe {
            // 1. Initialize Direct3D 11 device with BGRA support for Direct2D
            let mut d3d11_device = None;
            let mut feature_level = D3D_FEATURE_LEVEL::default();
            let mut immediate_context = None;

            let feature_levels = [
                D3D_FEATURE_LEVEL_11_1,
                D3D_FEATURE_LEVEL_11_0,
                D3D_FEATURE_LEVEL_10_1,
                D3D_FEATURE_LEVEL_10_0,
            ];

            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&feature_levels),
                D3D11_SDK_VERSION,
                Some(&mut d3d11_device),
                Some(&mut feature_level),
                Some(&mut immediate_context),
            )
            .context("D3D11CreateDevice failed for MPO")?;

            let d3d11_device = d3d11_device.unwrap();
            let dxgi_device: IDXGIDevice = d3d11_device
                .cast()
                .context("Failed to cast D3D11 device to IDXGIDevice")?;

            // 2. Retrieve DXGI adapter and factory
            let dxgi_adapter: IDXGIAdapter = dxgi_device
                .GetAdapter()
                .context("Failed to get DXGI adapter")?;
            let dxgi_factory: IDXGIFactory2 = dxgi_adapter
                .GetParent()
                .context("Failed to get DXGI factory")?;

            // 3. Create DirectComposition flip-discard swapchain for the overlay
            let swap_chain_desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: HUD_W,
                Height: HUD_H,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                Stereo: false.into(),
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                Flags: 0,
            };

            let swap_chain = dxgi_factory
                .CreateSwapChainForComposition(&d3d11_device, &swap_chain_desc, None)
                .context("CreateSwapChainForComposition failed")?;

            // 4. Initialize Direct2D & DirectWrite
            let d2d_factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)
                    .context("D2D1CreateFactory failed")?;
            let d2d_device: ID2D1Device = d2d_factory
                .CreateDevice(&dxgi_device)
                .context("D2D CreateDevice failed")?;
            let d2d_context: ID2D1DeviceContext = d2d_device
                .CreateDeviceContext(
                    windows::Win32::Graphics::Direct2D::D2D1_DEVICE_CONTEXT_OPTIONS_NONE,
                )
                .context("CreateDeviceContext failed")?;

            let render_target: ID2D1RenderTarget = d2d_context
                .cast()
                .context("Failed to cast D2D context to ID2D1RenderTarget")?;

            let dwrite_factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)
                .context("DWriteCreateFactory failed")?;

            let text_format = dwrite_factory
                .CreateTextFormat(
                    w!("Consolas"),
                    None,
                    DWRITE_FONT_WEIGHT_BOLD,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    12.5,
                    w!("en-us"),
                )
                .context("CreateTextFormat failed")?;

            let _ = text_format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
            let _ = text_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);

            // Pre-create solid brushes via ID2D1RenderTarget
            let brush_bg_dark = render_target
                .CreateSolidColorBrush(&COLOR_BG_DARK, None::<*const _>)
                .context("Failed to create bg dark brush")?;
            let brush_border_dark = render_target
                .CreateSolidColorBrush(&COLOR_BORDER_DARK, None::<*const _>)
                .context("Failed to create border dark brush")?;
            let brush_text_dark = render_target
                .CreateSolidColorBrush(&COLOR_TEXT_DARK, None::<*const _>)
                .context("Failed to create text dark brush")?;

            let brush_bg_light = render_target
                .CreateSolidColorBrush(&COLOR_BG_LIGHT, None::<*const _>)
                .context("Failed to create bg light brush")?;
            let brush_border_light = render_target
                .CreateSolidColorBrush(&COLOR_BORDER_LIGHT, None::<*const _>)
                .context("Failed to create border light brush")?;
            let brush_text_light = render_target
                .CreateSolidColorBrush(&COLOR_TEXT_LIGHT, None::<*const _>)
                .context("Failed to create text light brush")?;

            let brush_cobalt = render_target
                .CreateSolidColorBrush(&COLOR_COBALT, None::<*const _>)
                .context("Failed to create cobalt brush")?;
            let brush_amber = render_target
                .CreateSolidColorBrush(&COLOR_AMBER, None::<*const _>)
                .context("Failed to create amber brush")?;
            let brush_coral = render_target
                .CreateSolidColorBrush(&COLOR_CORAL, None::<*const _>)
                .context("Failed to create coral brush")?;

            // 5. Connect DirectComposition tree to HWND
            let dcomp_device: IDCompositionDevice = DCompositionCreateDevice(Some(&dxgi_device))
                .context("DCompositionCreateDevice failed")?;

            let dcomp_target = dcomp_device
                .CreateTargetForHwnd(hwnd, true)
                .context("CreateTargetForHwnd failed")?;

            let dcomp_visual = dcomp_device.CreateVisual().context("CreateVisual failed")?;

            dcomp_visual
                .SetContent(&swap_chain)
                .context("SetContent on visual failed")?;

            dcomp_target
                .SetRoot(&dcomp_visual)
                .context("SetRoot on target failed")?;

            dcomp_device
                .Commit()
                .context("DirectComposition Commit failed")?;

            Ok(Self {
                _d3d11_device: d3d11_device,
                _dcomp_device: dcomp_device,
                _dcomp_target: dcomp_target,
                _dcomp_visual: dcomp_visual,
                swap_chain,
                d2d_context,
                render_target,
                text_format,
                brush_bg_dark,
                brush_border_dark,
                brush_text_dark,
                brush_bg_light,
                brush_border_light,
                brush_text_light,
                brush_cobalt,
                brush_amber,
                brush_coral,
            })
        }
    }

    /// Renders a single frame directly to the DXGI flip swapchain and presents it.
    pub fn render_frame(
        &mut self,
        remaining_secs: i64,
        is_timer: bool,
        is_light: bool,
    ) -> Result<()> {
        unsafe {
            let back_buffer = self
                .swap_chain
                .GetBuffer::<windows::Win32::Graphics::Dxgi::IDXGISurface>(0)
                .context("GetBuffer(0) failed")?;

            let bitmap_properties = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                colorContext: ManuallyDrop::new(None),
            };

            let d2d_target_bitmap = self
                .d2d_context
                .CreateBitmapFromDxgiSurface(&back_buffer, Some(&bitmap_properties))
                .context("CreateBitmapFromDxgiSurface failed")?;

            self.d2d_context.SetTarget(&d2d_target_bitmap);
            self.render_target.BeginDraw();

            // Clear surface with complete transparency (0.0 alpha)
            self.render_target.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));

            // 1. Select palette brushes
            let bg_brush = if is_light {
                &self.brush_bg_light
            } else {
                &self.brush_bg_dark
            };
            let border_brush = if is_light {
                &self.brush_border_light
            } else {
                &self.brush_border_dark
            };
            let text_brush = if is_light {
                &self.brush_text_light
            } else {
                &self.brush_text_dark
            };
            let dot_brush = if remaining_secs <= 60 {
                &self.brush_coral
            } else if is_timer {
                &self.brush_amber
            } else {
                &self.brush_cobalt
            };

            // 2. Draw rounded solid pill background
            let rect = windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                left: 0.5,
                top: 0.5,
                right: HUD_W as f32 - 0.5,
                bottom: HUD_H as f32 - 0.5,
            };
            let rounded_rect = D2D1_ROUNDED_RECT {
                rect,
                radiusX: 13.0,
                radiusY: 13.0,
            };

            self.render_target
                .FillRoundedRectangle(&rounded_rect, bg_brush);

            // 3. Draw border
            self.render_target
                .DrawRoundedRectangle(&rounded_rect, border_brush, 1.0, None);

            // 4. Draw glowing status dot
            let dot_ellipse = D2D1_ELLIPSE {
                point: windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                    x: 14.0,
                    y: HUD_H as f32 / 2.0,
                },
                radiusX: 3.5,
                radiusY: 3.5,
            };
            self.render_target.FillEllipse(&dot_ellipse, dot_brush);

            // 5. Draw countdown digits with DirectWrite
            let hrs = remaining_secs / 3600;
            let mins = (remaining_secs % 3600) / 60;
            let secs = remaining_secs % 60;
            let text = if hrs > 0 {
                format!("{}:{:02}:{:02}", hrs, mins, secs)
            } else {
                format!("{:02}:{:02}", mins, secs)
            };
            let wide: Vec<u16> = text.encode_utf16().collect();

            let text_rect = windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                left: 23.0,
                top: 0.0,
                right: HUD_W as f32 - 6.0,
                bottom: HUD_H as f32,
            };

            self.render_target.DrawText(
                &wide,
                &self.text_format,
                &text_rect,
                text_brush,
                windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                windows::Win32::Graphics::DirectWrite::DWRITE_MEASURING_MODE_NATURAL,
            );

            self.render_target
                .EndDraw(None, None)
                .context("EndDraw failed")?;

            // Detach target bitmap before presenting
            self.d2d_context.SetTarget(None);

            // Present to hardware MPO plane without blocking (sync interval 0)
            self.swap_chain
                .Present(0, DXGI_PRESENT(0))
                .ok()
                .context("swap_chain.Present failed")?;

            Ok(())
        }
    }
}
