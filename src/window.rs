//! 面板窗口:无边框、置顶、不抢焦点、不入任务栏/Alt-Tab。
//!
//! 窗口过程把事件转发给 App(存于 GWLP_USERDATA):托盘回调、鼠标悬停、
//! 定时器(动画/轮播)、环境变化重定位、资源管理器重启重建托盘。

use crate::app::App;
use crate::win::{WM_APP_TRAY, WM_APP_UPDATE};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetWindowLongPtrW, LoadCursorW, PostQuitMessage,
    RegisterClassExW, SetWindowLongPtrW, CW_USEDEFAULT, GWLP_USERDATA, IDC_ARROW, WINDOW_EX_STYLE,
    WM_CREATE, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_MOUSEMOVE, WM_RBUTTONUP,
    WM_SETTINGCHANGE, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};

const CLASS_NAME: PCWSTR = windows::core::w!("cc_status_panel");

/// 面板窗口初始占位尺寸(像素)。M3 会按灯数量/DPI 重算。
const INIT_W: i32 = 140;
const INIT_H: i32 = 40;

/// 创建面板窗口。`app` 的所有权转移给窗口(存入 GWLP_USERDATA),窗口销毁时释放。
pub fn create(app: Box<App>) -> windows::core::Result<HWND> {
    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: CLASS_NAME,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH::default(),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let ex_style: WINDOW_EX_STYLE =
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED;

        let hwnd = CreateWindowExW(
            ex_style,
            CLASS_NAME,
            windows::core::w!("cc-status"),
            WS_POPUP,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            INIT_W,
            INIT_H,
            None,
            None,
            Some(hinstance.into()),
            Some(Box::into_raw(app) as *const _),
        )?;

        Ok(hwnd)
    }
}

/// 从窗口取回 App 引用。
unsafe fn app_from_hwnd<'a>(hwnd: HWND) -> Option<&'a mut App> {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut App;
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &mut *ptr })
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CREATE => {
                // 取出 CREATESTRUCT 中传入的 App 指针并存入 GWLP_USERDATA。
                let cs = lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW;
                if !cs.is_null() {
                    let app_ptr = (*cs).lpCreateParams as *mut App;
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, app_ptr as isize);
                    if let Some(app) = app_from_hwnd(hwnd) {
                        app.on_create(hwnd);
                    }
                }
                LRESULT(0)
            }
            WM_APP_TRAY => {
                // 托盘图标鼠标事件:lparam 低位是鼠标消息。
                let mouse = (lparam.0 & 0xFFFF) as u32;
                if mouse == WM_RBUTTONUP {
                    if let Some(app) = app_from_hwnd(hwnd) {
                        app.on_tray_context_menu();
                    }
                }
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                // 面板区域右键 → 同样弹设置菜单。
                if let Some(app) = app_from_hwnd(hwnd) {
                    app.on_tray_context_menu();
                }
                LRESULT(0)
            }
            WM_APP_UPDATE => {
                if let Some(app) = app_from_hwnd(hwnd) {
                    app.on_update();
                }
                LRESULT(0)
            }
            WM_TIMER => {
                if let Some(app) = app_from_hwnd(hwnd) {
                    app.on_timer(wparam.0);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                // 请求 WM_MOUSELEAVE 通知。
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                if let Some(app) = app_from_hwnd(hwnd) {
                    app.on_mouse_move(x, y);
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                if let Some(app) = app_from_hwnd(hwnd) {
                    app.on_mouse_leave();
                }
                LRESULT(0)
            }
            WM_SETTINGCHANGE | WM_DPICHANGED | WM_DISPLAYCHANGE => {
                if let Some(app) = app_from_hwnd(hwnd) {
                    app.on_reposition();
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                // 释放 App(取回 Box 并 drop)。
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
                if !ptr.is_null() {
                    drop(Box::from_raw(ptr));
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => {
                // TaskbarCreated 是运行时注册的动态消息 ID,只能在此比较。
                if msg == crate::win::taskbar_created_msg() {
                    if let Some(app) = app_from_hwnd(hwnd) {
                        app.on_taskbar_created();
                    }
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
    }
}

// 显示窗口时机由 App 控制(有灯时自行显示)。
// 退出通过菜单在 App 内直接 DestroyWindow 完成。
