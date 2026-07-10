//! 配置读写:`%APPDATA%\cc-status\config.json`。
//!
//! 对应需求文档 §6(配置持久化)。配置损坏时回退默认值并重建,不 panic。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 默认监听地址与端口。
pub const DEFAULT_IP: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 9898;

/// 心跳:超过此秒数无推送 → 置为 offline。
pub const DEFAULT_OFFLINE_TIMEOUT_SECS: u64 = 60;
/// offline 后再超过此秒数无推送 → 移除该灯。
pub const DEFAULT_REMOVE_TIMEOUT_SECS: u64 = 300;
/// 分屏轮播间隔(秒)。
pub const DEFAULT_CAROUSEL_INTERVAL_SECS: u64 = 4;

fn default_ip() -> String {
    DEFAULT_IP.to_string()
}
const fn default_port() -> u16 {
    DEFAULT_PORT
}
const fn default_offline_timeout() -> u64 {
    DEFAULT_OFFLINE_TIMEOUT_SECS
}
const fn default_remove_timeout() -> u64 {
    DEFAULT_REMOVE_TIMEOUT_SECS
}
const fn default_carousel_interval() -> u64 {
    DEFAULT_CAROUSEL_INTERVAL_SECS
}

/// 持久化配置。所有字段带默认值,前向兼容缺失字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 监听 IP。默认 127.0.0.1;可设为 0.0.0.0 或局域网 IP 以支持跨机器。
    #[serde(default = "default_ip")]
    pub listen_ip: String,
    /// 监听端口。
    #[serde(default = "default_port")]
    pub listen_port: u16,
    /// 是否开机自启(默认关闭)。
    #[serde(default)]
    pub autostart: bool,
    /// 可选共享 token;非空时请求需携带匹配的 X-CC-Token 头。
    #[serde(default)]
    pub token: String,
    /// 心跳超时(秒)。
    #[serde(default = "default_offline_timeout")]
    pub offline_timeout_secs: u64,
    /// 移除超时(秒)。
    #[serde(default = "default_remove_timeout")]
    pub remove_timeout_secs: u64,
    /// 分屏轮播间隔(秒)。
    #[serde(default = "default_carousel_interval")]
    pub carousel_interval_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_ip: default_ip(),
            listen_port: default_port(),
            autostart: false,
            token: String::new(),
            offline_timeout_secs: default_offline_timeout(),
            remove_timeout_secs: default_remove_timeout(),
            carousel_interval_secs: default_carousel_interval(),
        }
    }
}

impl Config {
    /// 监听地址是否为回环地址。
    pub fn is_loopback(&self) -> bool {
        self.listen_ip == "127.0.0.1" || self.listen_ip == "::1" || self.listen_ip == "localhost"
    }

    /// 配置文件目录:`%APPDATA%\cc-status`。
    pub fn config_dir() -> PathBuf {
        // APPDATA 在正常 Windows 会话下总是存在;缺失时退回当前目录。
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("cc-status")
    }

    /// 配置文件完整路径。
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    /// 从磁盘加载;文件不存在或损坏则返回默认值(并尝试写回默认文件)。
    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Config>(&text) {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!("[cc-status] 配置解析失败,回退默认值: {e}");
                    let cfg = Config::default();
                    let _ = cfg.save();
                    cfg
                }
            },
            Err(_) => {
                // 首次运行:生成默认配置。
                let cfg = Config::default();
                let _ = cfg.save();
                cfg
            }
        }
    }

    /// 写回磁盘(自动创建目录)。
    pub fn save(&self) -> anyhow::Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)?;
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(Self::config_path(), text)?;
        Ok(())
    }
}
