//! Win32 通用辅助:宽字符串转换、全局消息/命令常量。

use windows::core::w;
use windows::Win32::UI::WindowsAndMessaging::{RegisterWindowMessageW, WM_APP};

/// 托盘图标回调消息(鼠标事件经此送达窗口过程)。
pub const WM_APP_TRAY: u32 = WM_APP + 1;
/// HTTP 线程通知 UI 线程刷新(状态有变)。
pub const WM_APP_UPDATE: u32 = WM_APP + 2;

/// 托盘右键菜单命令 ID。
pub const ID_MENU_SETTINGS: usize = 1001;
pub const ID_MENU_EXIT: usize = 1002;

/// 资源管理器(explorer.exe)重启后会广播此消息,需据此重建托盘图标。
pub fn taskbar_created_msg() -> u32 {
    unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) }
}

/// Rust `&str` → 以 NUL 结尾的 UTF-16 缓冲(供 Win32 W 系列 API)。
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 把 `&str` 写入定长 UTF-16 数组(如 NOTIFYICONDATAW.szTip),自动截断并保证 NUL 结尾。
pub fn wide_into(dst: &mut [u16], s: &str) {
    let src: Vec<u16> = s.encode_utf16().collect();
    let n = src.len().min(dst.len().saturating_sub(1));
    dst[..n].copy_from_slice(&src[..n]);
    dst[n] = 0;
}
