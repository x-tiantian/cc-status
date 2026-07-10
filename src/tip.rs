//! 悬停提示窗口:一个自绘的深色文本气泡(需求 FR-4)。
//!
//! 独立的置顶、不抢焦点的小窗口;文本由 App 提供,GDI 绘制。

use crate::win::wide;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC, ReleaseDC,
    SelectObject, SetBkMode, SetTextColor, DT_CALCRECT, DT_LEFT, DT_NOPREFIX, DT_WORDBREAK,
    GetStockObject, DEFAULT_GUI_FONT, HFONT, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetWindowLongPtrW, LoadCursorW, RegisterClassExW,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, GWLP_USERDATA, HWND_TOPMOST, IDC_ARROW,
    SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE, WM_PAINT, WNDCLASSEXW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const TIP_CLASS: windows::core::PCWSTR = w!("cc_status_tip");
const PAD: i32 = 8; // 内边距
const MAX_W: i32 = 360; // 最大宽度(超出换行)

/// 提示窗口持有的状态(通过 GWLP_USERDATA 指针访问)。
pub struct TipState {
    pub text: String,
}

/// 创建(隐藏的)提示窗口。返回句柄;`state` 所有权交给窗口。
pub fn create(state: Box<TipState>) -> windows::core::Result<HWND> {
    unsafe {
        let hinst = GetModuleHandleW(None)?;
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(tip_proc),
            hInstance: hinst.into(),
            lpszClassName: TIP_CLASS,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            TIP_CLASS,
            w!("tip"),
            WS_POPUP,
            0,
            0,
            10,
            10,
            None,
            None,
            Some(hinst.into()),
            Some(Box::into_raw(state) as *const _),
        )?;
        Ok(hwnd)
    }
}

/// 更新文本、按内容测量尺寸、定位到 (anchor_x, anchor_y) 上方并显示。
pub fn show(hwnd: HWND, anchor_x: i32, anchor_y: i32) {
    unsafe {
        let (w, h) = measure(hwnd);
        // 放在锚点上方并水平居中;简单夹取避免越界(多屏精细化留待打磨)。
        let x = (anchor_x - w / 2).max(0);
        let y = (anchor_y - h - 6).max(0);
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        // 触发重绘。
        use windows::Win32::Graphics::Gdi::InvalidateRect;
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
}

/// 隐藏提示窗口。
pub fn hide(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

/// 依据当前文本测量窗口尺寸。
fn measure(hwnd: HWND) -> (i32, i32) {
    unsafe {
        let state = tip_state(hwnd);
        let text = state.map(|s| s.text.clone()).unwrap_or_default();
        let dc = GetDC(Some(hwnd));
        let font = GetStockObject(DEFAULT_GUI_FONT);
        let old = SelectObject(dc, font);
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: MAX_W - PAD * 2,
            bottom: 0,
        };
        let mut wtext = wide(&text);
        // 去掉结尾 NUL 以免 DrawText 计入。
        if wtext.last() == Some(&0) {
            wtext.pop();
        }
        DrawTextW(
            dc,
            &mut wtext,
            &mut rc,
            DT_CALCRECT | DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
        );
        SelectObject(dc, old);
        ReleaseDC(Some(hwnd), dc);
        let w = (rc.right - rc.left + PAD * 2).min(MAX_W).max(24);
        let h = (rc.bottom - rc.top + PAD * 2).max(20);
        let _ = SIZE { cx: w, cy: h };
        let _ = HFONT::default();
        (w, h)
    }
}

unsafe fn tip_state<'a>(hwnd: HWND) -> Option<&'a mut TipState> {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut TipState;
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &mut *ptr })
    }
}

extern "system" fn tip_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            windows::Win32::UI::WindowsAndMessaging::WM_CREATE => {
                let cs = lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW;
                if !cs.is_null() {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*cs).lpCreateParams as isize);
                }
                LRESULT(0)
            }
            WM_PAINT => {
                paint(hwnd);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn paint(hwnd: HWND) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);

        let mut rc = RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rc);

        // 深色背景。
        let bg = CreateSolidBrush(COLORREF(0x00202020));
        FillRect(hdc, &rc, bg);
        let _ = DeleteObject(bg.into());

        // 白色文字。
        let font = GetStockObject(DEFAULT_GUI_FONT);
        let old = SelectObject(hdc, font);
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(0x00FFFFFF));

        let text = tip_state(hwnd).map(|s| s.text.clone()).unwrap_or_default();
        let mut wtext = wide(&text);
        if wtext.last() == Some(&0) {
            wtext.pop();
        }
        let mut trc = RECT {
            left: rc.left + PAD,
            top: rc.top + PAD,
            right: rc.right - PAD,
            bottom: rc.bottom - PAD,
        };
        DrawTextW(
            hdc,
            &mut wtext,
            &mut trc,
            DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
        );

        SelectObject(hdc, old);
        let _ = EndPaint(hwnd, &ps);
    }
}
