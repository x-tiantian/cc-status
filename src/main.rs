//! cc-status:Windows 任务栏托盘旁的 Claude CLI 状态监控。
//!
//! M1 骨架:配置读写 + 无边框置顶窗口 + 系统托盘图标与右键菜单 + Win32 消息循环。
//! HTTP 监听(M2)、灯绘制与定位(M3)、分屏与 Tooltip(M4)、设置窗(M5)后续接入。

// 发布构建隐藏控制台窗口;调试构建保留控制台以便查看日志。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod config;
mod hook;
mod panel;
mod render;
mod server;
mod settings;
mod state;
mod status;
mod tip;
mod tray;
mod win;
mod window;

use app::App;
use config::Config;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, TranslateMessage, MSG,
};

fn main() -> anyhow::Result<()> {
    // 命令行工具:无界面地开关开机自启 / 打印 hook 配置(便于脚本调用)。
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        // GUI 子系统程序默认无控制台;附加到父进程控制台,使 CLI 输出可见。
        unsafe {
            use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
    match args.get(1).map(|s| s.as_str()) {
        Some("--enable-autostart") => {
            autostart::set(true)?;
            println!("开机自启:已启用");
            return Ok(());
        }
        Some("--disable-autostart") => {
            autostart::set(false)?;
            println!("开机自启:已关闭");
            return Ok(());
        }
        Some("--print-hooks") => {
            // 打印当前配置对应的 Claude Code hooks 片段(便于命令行获取)。
            let cfg = Config::load();
            println!(
                "{}",
                config::hooks_snippet(&cfg.listen_ip, cfg.listen_port, &cfg.token)
            );
            return Ok(());
        }
        _ => {}
    }

    // Per-Monitor DPI 感知,保证多显示器/缩放下的正确尺寸与定位(需求 §6)。
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let config = Config::load();
    let store = state::new_shared();

    // 创建面板窗口(拥有 App 所有权)。窗口在有灯时自行显示。
    let app = Box::new(App::new(config, store.clone()));
    let _hwnd = window::create(app).map_err(|e| anyhow::anyhow!("创建窗口失败: {e}"))?;

    // Win32 消息循环。
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    Ok(())
}
