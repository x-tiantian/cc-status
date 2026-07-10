//! 开机自启:通过 `HKCU\...\Run` 注册表项实现,用户级、无需管理员(需求 FR-7)。

use crate::win::wide;
use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
};

const RUN_KEY: PCWSTR =
    windows::core::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE_NAME: &str = "cc-status";

/// 当前可执行文件路径(带引号,避免空格路径问题)。
fn exe_path_quoted() -> String {
    let p = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("\"{p}\"")
}

/// 设置或取消开机自启。
pub fn set(enabled: bool) -> anyhow::Result<()> {
    unsafe {
        let mut hkey = HKEY::default();
        let rc = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            None,
            KEY_SET_VALUE | KEY_QUERY_VALUE,
            &mut hkey,
        );
        if rc != ERROR_SUCCESS {
            anyhow::bail!("打开注册表 Run 键失败: {rc:?}");
        }

        let name = wide(VALUE_NAME);
        let result = if enabled {
            // REG_SZ 数据需为以 NUL 结尾的宽字符字节序列。
            let data = wide(&exe_path_quoted());
            let bytes = std::slice::from_raw_parts(
                data.as_ptr() as *const u8,
                data.len() * std::mem::size_of::<u16>(),
            );
            RegSetValueExW(hkey, PCWSTR(name.as_ptr()), None, REG_SZ, Some(bytes))
        } else {
            let rc = RegDeleteValueW(hkey, PCWSTR(name.as_ptr()));
            // 值不存在视为成功(幂等)。
            if rc == ERROR_SUCCESS
                || rc == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND
            {
                ERROR_SUCCESS
            } else {
                rc
            }
        };

        let _ = RegCloseKey(hkey);
        if result != ERROR_SUCCESS {
            anyhow::bail!("写入注册表失败: {result:?}");
        }
    }
    Ok(())
}

/// 查询当前是否已设置开机自启(用于设置窗初始状态校正)。
pub fn is_enabled() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            None,
            KEY_QUERY_VALUE,
            &mut hkey,
        ) != ERROR_SUCCESS
        {
            return false;
        }
        let name = wide(VALUE_NAME);
        let mut ty = windows::Win32::System::Registry::REG_VALUE_TYPE::default();
        let mut size = 0u32;
        let rc = RegQueryValueExW(
            hkey,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut ty),
            None,
            Some(&mut size),
        );
        let _ = RegCloseKey(hkey);
        rc == ERROR_SUCCESS
    }
}
