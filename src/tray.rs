//! 系统托盘图标:基于 `Shell_NotifyIconW`。
//!
//! 承载右键菜单(设置 / 退出)与气泡提示,并作为面板定位失败时的永久入口
//! (对应需求文档 FR-8)。M1 使用系统默认图标,真实图标在 M7 替换。

use crate::win::{wide, wide_into, WM_APP_TRAY};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, LoadIconW, SetForegroundWindow,
    TrackPopupMenu, HICON, HMENU, IDI_APPLICATION, MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    TPM_RETURNCMD,
};

/// 托盘图标的唯一 ID(单图标固定为 1)。
const TRAY_UID: u32 = 1;

/// 托盘图标句柄封装。
pub struct Tray {
    hwnd: HWND,
    icon: HICON,
}

impl Tray {
    /// 在指定窗口上创建托盘图标。鼠标事件将以 `WM_APP_TRAY` 回调该窗口。
    pub fn new(hwnd: HWND, tip: &str) -> windows::core::Result<Self> {
        let icon = unsafe { LoadIconW(None, IDI_APPLICATION)? };
        let mut data = Self::base_data(hwnd, icon);
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = WM_APP_TRAY;
        wide_into(&mut data.szTip, tip);
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &data).ok()?;
        }
        Ok(Self { hwnd, icon })
    }

    fn base_data(hwnd: HWND, icon: HICON) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_UID,
            ..Default::default()
        };
        data.hIcon = icon;
        data
    }

    /// 更新悬停提示文字。
    pub fn set_tip(&self, tip: &str) {
        let mut data = Self::base_data(self.hwnd, self.icon);
        data.uFlags = NIF_TIP;
        wide_into(&mut data.szTip, tip);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }

    /// 在光标处弹出右键菜单,返回被点击的命令 ID(未选择返回 0)。
    pub fn show_context_menu(&self, items: &[(usize, &str)]) -> usize {
        unsafe {
            let menu: HMENU = match CreatePopupMenu() {
                Ok(m) => m,
                Err(_) => return 0,
            };
            for (id, text) in items {
                let _ = AppendMenuW(menu, MF_STRING, *id, windows::core::PCWSTR(wide(text).as_ptr()));
            }
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            // 必须先 SetForegroundWindow,否则菜单不会在点击外部时自动消失。
            let _ = SetForegroundWindow(self.hwnd);
            let cmd = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_LEFTALIGN | TPM_BOTTOMALIGN,
                pt.x,
                pt.y,
                Some(0),
                self.hwnd,
                None,
            );
            let _ = DestroyMenu(menu);
            cmd.0 as usize
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        let data = Self::base_data(self.hwnd, self.icon);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        }
    }
}
