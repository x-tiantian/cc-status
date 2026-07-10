//! 面板定位与呈现:计算托盘左侧锚定位置,并用 `UpdateLayeredWindow` 呈现灯。
//!
//! 对应需求 §3.2(定位)与 FR-3.6(降级右下角)。

use crate::render::Canvas;
use std::ffi::c_void;
use windows::core::w;
use windows::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, BI_RGB, DIB_RGB_COLORS,
    HBITMAP,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetWindowRect, SystemParametersInfoW, UpdateLayeredWindow,
    SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, ULW_ALPHA,
};

/// 每屏最多显示的灯数量(需求 FR-3.4)。
pub const MAX_PER_PAGE: usize = 3;

/// 96 DPI 下的基准尺寸(像素)。
const BASE_DOT: f32 = 16.0; // 灯直径
const BASE_GAP: f32 = 12.0; // 灯间距
const BASE_PAD: f32 = 12.0; // 面板内边距
const BASE_H: f32 = 40.0; // 面板高度

/// 面板在屏幕上的位置与尺寸。
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// 布局结果:面板尺寸 + 每个灯的中心坐标(面板内相对像素)+ 灯半径。
pub struct Layout {
    pub w: i32,
    pub h: i32,
    pub centers: Vec<(f32, f32)>,
    pub radius: f32,
}

/// 当前窗口 DPI 缩放系数。
pub fn dpi_scale(hwnd: HWND) -> f32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 }
}

/// 计算显示 `n` 个灯所需的面板布局(横向一排)。
pub fn layout(n: usize, scale: f32) -> Layout {
    let n = n.max(1);
    let dot = BASE_DOT * scale;
    let gap = BASE_GAP * scale;
    let pad = BASE_PAD * scale;
    let h = BASE_H * scale;
    let radius = dot / 2.0;

    let w = pad * 2.0 + n as f32 * dot + (n as f32 - 1.0) * gap;
    let cy = h / 2.0;
    let mut centers = Vec::with_capacity(n);
    for i in 0..n {
        let cx = pad + radius + i as f32 * (dot + gap);
        centers.push((cx, cy));
    }
    Layout {
        w: w.ceil() as i32,
        h: h.ceil() as i32,
        centers,
        radius,
    }
}

/// 计算面板应放置的屏幕位置:优先锚定托盘左侧,失败则降级右下角。
pub fn compute_placement(panel_w: i32, panel_h: i32) -> Placement {
    if let Some(p) = anchor_to_tray(panel_w, panel_h) {
        return p;
    }
    fallback_bottom_right(panel_w, panel_h)
}

/// 锚定到系统托盘(通知区域)左侧。
fn anchor_to_tray(panel_w: i32, panel_h: i32) -> Option<Placement> {
    unsafe {
        let tray = FindWindowW(w!("Shell_TrayWnd"), None).ok()?;
        let mut tb = RECT::default();
        GetWindowRect(tray, &mut tb).ok()?;

        let notify = FindWindowExW(Some(tray), None, w!("TrayNotifyWnd"), None).ok()?;
        let mut nr = RECT::default();
        GetWindowRect(notify, &mut nr).ok()?;

        let tb_w = tb.right - tb.left;
        let tb_h = tb.bottom - tb.top;
        let horizontal = tb_w >= tb_h;

        let (x, y) = if horizontal {
            // 底部/顶部任务栏:面板右缘贴通知区左缘,竖直居中于任务栏。
            let x = nr.left - panel_w;
            let y = tb.top + (tb_h - panel_h) / 2;
            (x, y)
        } else {
            // 左/右侧竖直任务栏:面板置于通知区上方,水平居中于任务栏。
            let x = tb.left + (tb_w - panel_w) / 2;
            let y = nr.top - panel_h;
            (x, y)
        };
        Some(Placement {
            x,
            y,
            w: panel_w,
            h: panel_h,
        })
    }
}

/// 降级:屏幕工作区右下角。
fn fallback_bottom_right(panel_w: i32, panel_h: i32) -> Placement {
    let mut wa = RECT::default();
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut wa as *mut _ as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_ok()
    };
    let margin = 8;
    if ok {
        Placement {
            x: wa.right - panel_w - margin,
            y: wa.bottom - panel_h - margin,
            w: panel_w,
            h: panel_h,
        }
    } else {
        Placement {
            x: 100,
            y: 100,
            w: panel_w,
            h: panel_h,
        }
    }
}

/// 重新把面板抢占到置顶层,避免被任务栏(同为置顶窗口)激活后盖住。
/// 保持位置与尺寸不变、不抢焦点。
pub fn reassert_topmost(hwnd: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

/// 把画布通过 `UpdateLayeredWindow` 呈现到屏幕。
pub fn present(hwnd: HWND, placement: Placement, canvas: &Canvas) {
    unsafe {
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return;
        }
        let mem_dc = CreateCompatibleDC(Some(screen_dc));

        // 自上而下 32bpp DIB。
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: canvas.width,
                biHeight: -canvas.height, // 负数 = top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits: *mut c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(
            Some(mem_dc),
            &mut bmi,
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        );

        let dib: HBITMAP = match dib {
            Ok(b) if !bits.is_null() => b,
            _ => {
                let _ = DeleteDC(mem_dc);
                ReleaseDC(None, screen_dc);
                return;
            }
        };

        // 拷贝像素到 DIB。
        std::ptr::copy_nonoverlapping(
            canvas.pixels.as_ptr(),
            bits as *mut u32,
            canvas.pixels.len(),
        );

        let old = SelectObject(mem_dc, dib.into());

        let mut dst = POINT {
            x: placement.x,
            y: placement.y,
        };
        let mut src = POINT { x: 0, y: 0 };
        let mut size = SIZE {
            cx: canvas.width,
            cy: canvas.height,
        };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        let r = UpdateLayeredWindow(
            hwnd,
            Some(screen_dc),
            Some(&mut dst),
            Some(&mut size),
            Some(mem_dc),
            Some(&mut src),
            windows::Win32::Foundation::COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
        if r.is_err() {
            eprintln!("[cc-status] UpdateLayeredWindow 失败: {r:?}");
        }

        SelectObject(mem_dc, old);
        let _ = DeleteObject(dib.into());
        let _ = DeleteDC(mem_dc);
        ReleaseDC(None, screen_dc);
    }
}
