//! 应用状态:串联配置、托盘、状态存储、HTTP 服务与窗口事件。

use crate::config::Config;
use crate::panel::{self, Layout, MAX_PER_PAGE};
use crate::render::Canvas;
use crate::server::{self, ServerConfig, ServerCtrl};
use crate::state::{Light, LightKey, SharedStore};
use crate::tip::{self, TipState};
use crate::tray::Tray;
use crate::win::{ID_MENU_EXIT, ID_MENU_SETTINGS, WM_APP_UPDATE};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    KillTimer, PostMessageW, SetTimer, ShowWindow, SW_HIDE, SW_SHOWNOACTIVATE,
};

/// 动画重绘定时器 ID 与间隔(约 60ms,足够呼吸/闪烁流畅且省电)。
pub const ANIM_TIMER_ID: usize = 1;
const ANIM_INTERVAL_MS: u32 = 60;
/// 分屏轮播定时器 ID。
pub const CAROUSEL_TIMER_ID: usize = 2;
/// 置顶保持定时器 ID:周期性重新抢占置顶,防止被任务栏盖住。
pub const TOPMOST_TIMER_ID: usize = 3;
const TOPMOST_INTERVAL_MS: u32 = 400;

/// 应用运行期状态。由窗口过程通过 GWLP_USERDATA 持有。
pub struct App {
    pub config: Config,
    pub store: SharedStore,
    hwnd: HWND,
    tray: Option<Tray>,
    /// 悬停提示窗口。
    tip_hwnd: HWND,
    /// HTTP 服务停止信号发送端(重启时先停旧线程)。
    server_ctrl: Option<Sender<ServerCtrl>>,
    server_handle: Option<std::thread::JoinHandle<()>>,
    /// 动画起点,用于计算相位。
    anim_start: Instant,
    /// 动画定时器是否已开启。
    timer_on: bool,
    /// 轮播定时器是否已开启。
    carousel_on: bool,
    /// 置顶保持定时器是否已开启。
    topmost_on: bool,
    /// 当前是否有灯可见。
    visible: bool,
    /// 当前显示页(0 基)。
    page: usize,
    /// 当前页展示的灯(按显示顺序),供命中测试使用。
    shown: Vec<(LightKey, Light)>,
    /// 当前页布局几何(灯中心相对面板像素 + 半径)。
    centers: Vec<(f32, f32)>,
    radius: f32,
    /// 当前面板左上角屏幕坐标(把客户区坐标换算为屏幕坐标)。
    panel_x: i32,
    panel_y: i32,
    /// 鼠标是否悬停在面板上(悬停时暂停轮播)。
    hovering: bool,
    /// 当前悬停命中的灯索引。
    hover_index: Option<usize>,
}

impl App {
    pub fn new(config: Config, store: SharedStore) -> Self {
        Self {
            config,
            store,
            hwnd: HWND::default(),
            tray: None,
            tip_hwnd: HWND::default(),
            server_ctrl: None,
            server_handle: None,
            anim_start: Instant::now(),
            timer_on: false,
            carousel_on: false,
            topmost_on: false,
            visible: false,
            page: 0,
            shown: Vec::new(),
            centers: Vec::new(),
            radius: 0.0,
            panel_x: 0,
            panel_y: 0,
            hovering: false,
            hover_index: None,
        }
    }

    /// 托盘提示文字:显示监听地址与当前灯数量。
    fn tray_tip(&self) -> String {
        let n = self.store.lock().map(|s| s.len()).unwrap_or(0);
        format!(
            "cc-status · {}:{} · {} 个项目",
            self.config.listen_ip, self.config.listen_port, n
        )
    }

    /// 窗口创建完成:建立托盘图标、提示窗口并启动后台服务。
    pub fn on_create(&mut self, hwnd: HWND) {
        self.hwnd = hwnd;
        match Tray::new(hwnd, &self.tray_tip()) {
            Ok(tray) => self.tray = Some(tray),
            Err(e) => eprintln!("[cc-status] 创建托盘图标失败: {e}"),
        }
        match tip::create(Box::new(TipState { text: String::new() })) {
            Ok(h) => self.tip_hwnd = h,
            Err(e) => eprintln!("[cc-status] 创建提示窗口失败: {e}"),
        }
        self.start_server();
        self.start_sweeper();
        self.redraw();
    }

    /// 启动 HTTP 监听线程(按当前配置)。
    fn start_server(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        let cfg = ServerConfig {
            ip: self.config.listen_ip.clone(),
            port: self.config.listen_port,
            token: self.config.token.clone(),
        };
        let handle = server::spawn(cfg, self.store.clone(), self.hwnd.0 as isize, rx);
        self.server_ctrl = Some(tx);
        self.server_handle = Some(handle);
    }

    /// 停止当前 HTTP 监听线程(用于重启)。
    pub fn stop_server(&mut self) {
        if let Some(tx) = self.server_ctrl.take() {
            let _ = tx.send(ServerCtrl::Stop);
        }
        if let Some(h) = self.server_handle.take() {
            let _ = h.join();
        }
    }

    /// 按新配置重启监听(M5 设置保存后调用)。
    pub fn restart_server(&mut self) {
        self.stop_server();
        self.start_server();
        self.on_update();
    }

    /// 启动心跳清理线程:定期 sweep,变化时通知 UI。
    fn start_sweeper(&self) {
        let store = self.store.clone();
        let hwnd_raw = self.hwnd.0 as isize;
        let offline_after = Duration::from_secs(self.config.offline_timeout_secs);
        let remove_after =
            Duration::from_secs(self.config.offline_timeout_secs + self.config.remove_timeout_secs);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            let changed = store
                .lock()
                .map(|mut s| s.sweep(offline_after, remove_after))
                .unwrap_or(false);
            if changed && hwnd_raw != 0 {
                unsafe {
                    let _ = PostMessageW(
                        Some(HWND(hwnd_raw as *mut _)),
                        WM_APP_UPDATE,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            }
        });
    }

    /// 重绘面板。计算分页/布局/定位并呈现;按需开关动画与轮播定时器。
    pub fn redraw(&mut self) {
        let all = self.store.lock().map(|s| s.snapshot()).unwrap_or_default();

        if all.is_empty() {
            self.shown.clear();
            self.hide();
            self.set_anim_timer(false);
            self.set_carousel_timer(false);
            tip::hide(self.tip_hwnd);
            return;
        }

        // 分页(每页 MAX_PER_PAGE 个)。
        let pages = all.len().div_ceil(MAX_PER_PAGE);
        if self.page >= pages {
            self.page = 0;
        }
        let start = self.page * MAX_PER_PAGE;
        let end = (start + MAX_PER_PAGE).min(all.len());
        self.shown = all[start..end].to_vec();

        // 布局与绘制。
        let scale = panel::dpi_scale(self.hwnd);
        let lay: Layout = panel::layout(self.shown.len(), scale);
        self.centers = lay.centers.clone();
        self.radius = lay.radius;

        let mut canvas = Canvas::new(lay.w, lay.h);
        let phase = self.anim_start.elapsed().as_secs_f32();
        let mut need_anim = false;
        for (i, (_, light)) in self.shown.iter().enumerate() {
            let (cx, cy) = lay.centers[i];
            let anim = light.status.animation();
            if anim.is_animated() {
                need_anim = true;
            }
            canvas.draw_dot(cx, cy, lay.radius, light.status.color(), anim.intensity(phase));
        }

        let placement = panel::compute_placement(lay.w, lay.h);
        self.panel_x = placement.x;
        self.panel_y = placement.y;
        panel::present(self.hwnd, placement, &canvas);
        self.show();

        self.set_anim_timer(need_anim);
        // 多页且未悬停时启用轮播。
        self.set_carousel_timer(pages > 1 && !self.hovering);
    }

    fn show(&mut self) {
        if !self.visible {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
            self.visible = true;
            self.set_topmost_timer(true);
        }
        // 每次呈现后立即抢占置顶,确保压在任务栏之上。
        panel::reassert_topmost(self.hwnd);
    }

    fn hide(&mut self) {
        if self.visible {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
            self.set_topmost_timer(false);
        }
    }

    /// 开关"置顶保持"定时器(可见期间持续运行,周期性重新置顶)。
    fn set_topmost_timer(&mut self, on: bool) {
        if on && !self.topmost_on {
            unsafe {
                SetTimer(Some(self.hwnd), TOPMOST_TIMER_ID, TOPMOST_INTERVAL_MS, None);
            }
            self.topmost_on = true;
        } else if !on && self.topmost_on {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), TOPMOST_TIMER_ID);
            }
            self.topmost_on = false;
        }
    }

    /// 开关动画定时器(仅在有动画灯时运行,空闲不空转)。
    fn set_anim_timer(&mut self, on: bool) {
        if on && !self.timer_on {
            unsafe {
                SetTimer(Some(self.hwnd), ANIM_TIMER_ID, ANIM_INTERVAL_MS, None);
            }
            self.timer_on = true;
        } else if !on && self.timer_on {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), ANIM_TIMER_ID);
            }
            self.timer_on = false;
        }
    }

    /// 开关轮播定时器。
    fn set_carousel_timer(&mut self, on: bool) {
        if on && !self.carousel_on {
            let ms = (self.config.carousel_interval_secs.max(1) * 1000) as u32;
            unsafe {
                SetTimer(Some(self.hwnd), CAROUSEL_TIMER_ID, ms, None);
            }
            self.carousel_on = true;
        } else if !on && self.carousel_on {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), CAROUSEL_TIMER_ID);
            }
            self.carousel_on = false;
        }
    }

    /// 定时器触发。区分动画帧、轮播翻页、置顶保持。
    pub fn on_timer(&mut self, id: usize) {
        if id == TOPMOST_TIMER_ID {
            // 轻量:仅重新抢占置顶,不整帧重绘。
            if self.visible {
                panel::reassert_topmost(self.hwnd);
            }
            return;
        }
        if id == CAROUSEL_TIMER_ID {
            let n = self.store.lock().map(|s| s.len()).unwrap_or(0);
            let pages = n.div_ceil(MAX_PER_PAGE).max(1);
            self.page = (self.page + 1) % pages;
        }
        self.redraw();
    }

    /// 环境变化(任务栏移动 / DPI / 分辨率):重新定位并重绘。
    pub fn on_reposition(&mut self) {
        self.redraw();
    }

    /// 资源管理器重启:重建托盘图标并重绘。
    pub fn on_taskbar_created(&mut self) {
        match Tray::new(self.hwnd, &self.tray_tip()) {
            Ok(tray) => self.tray = Some(tray),
            Err(e) => eprintln!("[cc-status] 重建托盘图标失败: {e}"),
        }
        self.redraw();
    }

    /// 鼠标在面板上移动:命中测试并显示/更新提示;悬停时暂停轮播。
    pub fn on_mouse_move(&mut self, cx: i32, cy: i32) {
        if !self.hovering {
            self.hovering = true;
            self.set_carousel_timer(false); // 悬停暂停轮播
        }
        let hit = self.hit_test(cx as f32, cy as f32);
        if hit == self.hover_index {
            return; // 未变化
        }
        self.hover_index = hit;
        match hit {
            Some(i) => {
                let text = self.tip_text(i);
                if let Some(state) = self.tip_state_mut() {
                    state.text = text;
                }
                // 锚点:命中灯的屏幕坐标(面板左上 + 灯中心)。
                let (lx, ly) = self.centers[i];
                let ax = self.panel_x + lx as i32;
                let ay = self.panel_y + ly as i32 - self.radius as i32;
                tip::show(self.tip_hwnd, ax, ay);
            }
            None => tip::hide(self.tip_hwnd),
        }
    }

    /// 鼠标离开面板:隐藏提示,恢复轮播。
    pub fn on_mouse_leave(&mut self) {
        self.hovering = false;
        self.hover_index = None;
        tip::hide(self.tip_hwnd);
        // 若为多页则恢复轮播。
        let n = self.store.lock().map(|s| s.len()).unwrap_or(0);
        self.set_carousel_timer(n.div_ceil(MAX_PER_PAGE) > 1);
    }

    /// 命中测试:返回光标(面板相对像素)覆盖的灯索引。
    fn hit_test(&self, x: f32, y: f32) -> Option<usize> {
        let r = self.radius + 2.0; // 略放宽命中范围
        for (i, (cx, cy)) in self.centers.iter().enumerate() {
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= r * r {
                return Some(i);
            }
        }
        None
    }

    /// 组装提示文本:项目(含主机)+ 状态 + 消息。
    fn tip_text(&self, i: usize) -> String {
        let (key, light) = &self.shown[i];
        let title = if key.host.is_empty() {
            key.project.clone()
        } else {
            format!("{}/{}", key.host, key.project)
        };
        let status_line = light.status.label_zh();
        if light.message.is_empty() {
            format!("{title}\n{status_line}")
        } else {
            format!("{title}\n{status_line} · {}", light.message)
        }
    }

    fn tip_state_mut(&self) -> Option<&mut TipState> {
        if self.tip_hwnd.0.is_null() {
            return None;
        }
        unsafe {
            let ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                self.tip_hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            ) as *mut TipState;
            if ptr.is_null() { None } else { Some(&mut *ptr) }
        }
    }

    /// 右键 → 弹出上下文菜单并处理选择。
    pub fn on_tray_context_menu(&mut self) {
        let cmd = match &self.tray {
            Some(tray) => {
                tray.show_context_menu(&[(ID_MENU_SETTINGS, "设置…"), (ID_MENU_EXIT, "退出")])
            }
            None => return,
        };
        match cmd {
            ID_MENU_SETTINGS => self.open_settings(),
            ID_MENU_EXIT => {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.hwnd);
                }
                return;
            }
            _ => {}
        }
        // 菜单关闭后恢复面板输入层级与 hover 状态。
        self.after_popup();
    }

    /// 任何抢焦点的弹窗(右键菜单 / 设置窗)关闭后调用:
    /// 重置 hover 状态并重新抢占置顶,恢复 tooltip 与右键响应。
    fn after_popup(&mut self) {
        self.hovering = false;
        self.hover_index = None;
        crate::tip::hide(self.tip_hwnd);
        if self.visible {
            panel::reassert_topmost(self.hwnd);
        }
    }

    /// 打开设置窗;保存后按需重启监听、刷新界面。
    fn open_settings(&mut self) {
        // 悬停提示先隐藏,避免遮挡。
        crate::tip::hide(self.tip_hwnd);
        let outcome = crate::settings::show_modal(self.hwnd, &self.config);
        if outcome.quit {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.hwnd);
            }
            return;
        }
        if let Some(new_cfg) = outcome.new_config {
            let need_restart = new_cfg.listen_ip != self.config.listen_ip
                || new_cfg.listen_port != self.config.listen_port
                || new_cfg.token != self.config.token;
            self.config = new_cfg;
            if let Err(e) = self.config.save() {
                eprintln!("[cc-status] 保存配置失败: {e}");
            }
            if need_restart {
                self.restart_server();
            } else {
                self.on_update();
            }
        }
        // 设置窗关闭后恢复面板输入层级与 hover 状态。
        self.after_popup();
    }

    /// 收到状态更新通知:刷新托盘提示并重绘面板。
    pub fn on_update(&mut self) {
        if let Some(tray) = &self.tray {
            tray.set_tip(&self.tray_tip());
        }
        self.redraw();
    }
}
