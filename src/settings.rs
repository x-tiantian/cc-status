//! 设置窗口:修改监听 IP/端口、开机自启、token,并可退出程序(需求 FR-5)。
//!
//! 用代码创建的子控件(标签 + 编辑框 + 复选框 + 按钮)组成,避免依赖资源编译器。

use crate::autostart;
use crate::config::Config;
use crate::win::{wide, wide_into};
use std::cell::RefCell;
use windows::core::{PCWSTR, w};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT, HFONT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{BST_CHECKED, BST_UNCHECKED};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowTextW, IsWindow, RegisterClassExW, SendMessageW, SetWindowTextW, ShowWindow,
    TranslateMessage, BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, CW_USEDEFAULT,
    ES_AUTOHSCROLL, MSG, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_DESTROY,
    WM_SETFONT, WNDCLASSEXW, WS_BORDER, WS_CHILD, WS_EX_DLGMODALFRAME, WS_OVERLAPPED, WS_SYSMENU,
    WS_TABSTOP, WS_VISIBLE,
};

const CLASS: PCWSTR = w!("cc_status_settings");

// 控件 ID。
const ID_IP: i32 = 101;
const ID_PORT: i32 = 102;
const ID_TOKEN: i32 = 103;
const ID_AUTOSTART: i32 = 104;
const ID_SAVE: i32 = 105;
const ID_CANCEL: i32 = 106;
const ID_QUIT: i32 = 107;

thread_local! {
    /// 设置窗内部工作状态(单例,模态期间有效)。
    static CTX: RefCell<Option<Ctx>> = const { RefCell::new(None) };
}

struct Ctx {
    ip: HWND,
    port: HWND,
    token: HWND,
    autostart: HWND,
    /// 输入的初始配置副本;保存时据此产出新配置。
    original: Config,
    /// 保存结果:Some(新配置) 表示用户点击了保存。
    result: Option<Config>,
    /// 是否请求退出程序。
    quit: bool,
    done: bool,
}

/// 以模态方式显示设置窗。返回:
/// - `Ok(Some(cfg))`:用户保存,返回新配置(自启已在此函数内落地)。
/// - `Ok(None)`:用户取消或关闭。
/// - quit 标志通过返回的 `SettingsOutcome` 传达。
pub struct SettingsOutcome {
    pub new_config: Option<Config>,
    pub quit: bool,
}

pub fn show_modal(owner: HWND, current: &Config) -> SettingsOutcome {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc),
            hInstance: hinst.into(),
            lpszClassName: CLASS,
            hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(
                (windows::Win32::Graphics::Gdi::COLOR_BTNFACE.0 + 1) as *mut _,
            ),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let style = WS_OVERLAPPED | WS_SYSMENU;
        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            CLASS,
            w!("cc-status 设置"),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            360,
            250,
            Some(owner),
            None,
            Some(hinst.into()),
            None,
        )
        .unwrap();

        build_controls(hwnd, current);

        // 模态:禁用 owner,跑本地消息循环直到 done。
        if !owner.0.is_null() {
            let _ = EnableWindow(owner, false);
        }
        let _ = ShowWindow(hwnd, SW_SHOW);

        let mut msg = MSG::default();
        loop {
            let done = CTX.with(|c| c.borrow().as_ref().map(|x| x.done).unwrap_or(true));
            if done {
                break;
            }
            let r = GetMessageW(&mut msg, None, 0, 0);
            if !r.as_bool() {
                break;
            }
            // 简单的对话框式键盘处理可后续增强;此处直接分发。
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if !owner.0.is_null() {
            let _ = EnableWindow(owner, true);
        }
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = DestroyWindow(hwnd);
        }

        let (new_config, quit) = CTX.with(|c| {
            let mut b = c.borrow_mut();
            let out = b
                .as_ref()
                .map(|x| (x.result.clone(), x.quit))
                .unwrap_or((None, false));
            *b = None;
            out
        });
        SettingsOutcome { new_config, quit }
    }
}

fn build_controls(hwnd: HWND, cfg: &Config) {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
        let font = GetStockObject(DEFAULT_GUI_FONT);

        let label = |text: PCWSTR, x, y, w, h| {
            let c = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                text,
                WS_CHILD | WS_VISIBLE,
                x,
                y,
                w,
                h,
                Some(hwnd),
                None,
                Some(hinst.into()),
                None,
            )
            .unwrap();
            set_font(c, font);
            c
        };
        let edit = |id: i32, x, y, w, h, ex: WINDOW_STYLE| {
            let c = CreateWindowExW(
                WS_EX_DLGMODALFRAME,
                w!("EDIT"),
                w!(""),
                WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP | ex,
                x,
                y,
                w,
                h,
                Some(hwnd),
                Some(windows::Win32::UI::WindowsAndMessaging::HMENU(id as isize as *mut _)),
                Some(hinst.into()),
                None,
            )
            .unwrap();
            set_font(c, font);
            c
        };

        label(w!("监听 IP:"), 16, 18, 80, 20);
        let ip = edit(ID_IP, 100, 16, 230, 24, WINDOW_STYLE(ES_AUTOHSCROLL as u32));
        set_text(ip, &cfg.listen_ip);

        label(w!("端口:"), 16, 52, 80, 20);
        let port = edit(ID_PORT, 100, 50, 230, 24, WINDOW_STYLE(ES_AUTOHSCROLL as u32));
        set_text(port, &cfg.listen_port.to_string());

        label(w!("Token(可选):"), 16, 86, 80, 20);
        let token = edit(ID_TOKEN, 100, 84, 230, 24, WINDOW_STYLE(ES_AUTOHSCROLL as u32));
        set_text(token, &cfg.token);

        // 复选框:开机自启。
        let autostart = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            w!("开机启动"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            100,
            118,
            200,
            22,
            Some(hwnd),
            Some(windows::Win32::UI::WindowsAndMessaging::HMENU(ID_AUTOSTART as isize as *mut _)),
            Some(hinst.into()),
            None,
        )
        .unwrap();
        set_font(autostart, font);
        // 初始勾选状态以注册表实际情况为准。
        let checked = cfg.autostart || autostart::is_enabled();
        SendMessageW(
            autostart,
            BM_SETCHECK,
            Some(WPARAM(if checked { BST_CHECKED.0 as usize } else { BST_UNCHECKED.0 as usize })),
            None,
        );

        // 按钮。
        let button = |id: i32, text: PCWSTR, x, y, def: bool| {
            let extra = if def { BS_DEFPUSHBUTTON as u32 } else { 0 };
            let c = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("BUTTON"),
                text,
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(extra),
                x,
                y,
                80,
                28,
                Some(hwnd),
                Some(windows::Win32::UI::WindowsAndMessaging::HMENU(id as isize as *mut _)),
                Some(hinst.into()),
                None,
            )
            .unwrap();
            set_font(c, font);
            c
        };
        button(ID_SAVE, w!("保存"), 60, 160, true);
        button(ID_CANCEL, w!("取消"), 150, 160, false);
        button(ID_QUIT, w!("退出程序"), 240, 160, false);

        CTX.with(|c| {
            *c.borrow_mut() = Some(Ctx {
                ip,
                port,
                token,
                autostart,
                original: cfg.clone(),
                result: None,
                quit: false,
                done: false,
            });
        });
    }
}

fn set_font(hwnd: HWND, font: windows::Win32::Graphics::Gdi::HGDIOBJ) {
    unsafe {
        SendMessageW(
            hwnd,
            WM_SETFONT,
            Some(WPARAM(HFONT(font.0).0 as usize)),
            Some(LPARAM(1)),
        );
    }
}

fn set_text(hwnd: HWND, text: &str) {
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(wide(text).as_ptr()));
    }
}

fn get_text(hwnd: HWND) -> String {
    unsafe {
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_COMMAND => {
                let id = (wparam.0 & 0xFFFF) as i32;
                match id {
                    ID_SAVE => on_save(),
                    ID_CANCEL => finish(false, false),
                    ID_QUIT => finish(false, true),
                    _ => {}
                }
                LRESULT(0)
            }
            // 点击右上角 × 或按 Alt+F4:等同"取消",统一走结束逻辑。
            WM_CLOSE => {
                finish(false, false);
                LRESULT(0)
            }
            // 兜底:无论何种途径销毁,都标记结束,避免模态循环卡死。
            WM_DESTROY => {
                finish(false, false);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn on_save() {
    CTX.with(|c| {
        let mut b = c.borrow_mut();
        let ctx = match b.as_mut() {
            Some(x) => x,
            None => return,
        };
        let ip = get_text(ctx.ip);
        let port_str = get_text(ctx.port);
        let token = get_text(ctx.token);
        let checked = unsafe {
            SendMessageW(ctx.autostart, BM_GETCHECK, None, None).0 == BST_CHECKED.0 as isize
        };

        let port: u16 = match port_str.trim().parse() {
            Ok(p) if p > 0 => p,
            _ => {
                msgbox("端口必须是 1..65535 的数字。");
                return;
            }
        };
        let ip = ip.trim().to_string();
        if ip.is_empty() {
            msgbox("监听 IP 不能为空(本机用 127.0.0.1)。");
            return;
        }
        // 非回环给出安全提示。
        let mut cfg = ctx.original.clone();
        cfg.listen_ip = ip;
        cfg.listen_port = port;
        cfg.token = token.trim().to_string();
        cfg.autostart = checked;

        if !cfg.is_loopback() && cfg.token.is_empty() {
            msgbox("提示:监听非回环地址会对局域网开放,建议设置 Token。仍将保存。");
        }

        // 落地开机自启。
        if let Err(e) = autostart::set(checked) {
            msgbox(&format!("设置开机自启失败:{e}"));
        }

        ctx.result = Some(cfg);
        ctx.done = true;
    });
}

fn finish(_save: bool, quit: bool) {
    CTX.with(|c| {
        if let Some(ctx) = c.borrow_mut().as_mut() {
            ctx.quit = quit;
            ctx.done = true;
        }
    });
}

fn msgbox(text: &str) {
    unsafe {
        let mut t = [0u16; 512];
        wide_into(&mut t, text);
        windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
            None,
            PCWSTR(t.as_ptr()),
            w!("cc-status"),
            windows::Win32::UI::WindowsAndMessaging::MB_OK,
        );
    }
}
