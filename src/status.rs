//! 项目状态枚举及其视觉映射(颜色 / 动画)。
//!
//! 状态集合与配色对应需求文档 §5.1。颜色为初步方案,后续可在打磨阶段微调。

use serde::{Deserialize, Serialize};

/// 一个项目(Claude CLI 实例)当前所处的工作状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// 空闲 / 任务完成待命。
    Idle,
    /// 正在思考 / 推理(收到输入或工具结果后、执行工具前的推理阶段)。
    Thinking,
    /// 正在执行工具 / 工作中。
    Working,
    /// 等待用户输入 / 对话。
    WaitingInput,
    /// 等待用户授权。
    WaitingPermission,
    /// 出错(如 API 错误导致回合终止)。
    Error,
    /// 超时无心跳。
    Offline,
}

/// 灯的动画方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Animation {
    /// 常亮,不刷新。
    Solid,
    /// 呼吸(亮度缓慢正弦起伏)。
    Breathe,
    /// 闪烁(醒目,提示需要介入)。
    Blink,
}

impl Animation {
    /// 是否需要定时重绘。
    pub const fn is_animated(self) -> bool {
        !matches!(self, Animation::Solid)
    }

    /// 给定累计相位(秒),返回当前亮度系数 0.0..=1.0。
    pub fn intensity(self, phase: f32) -> f32 {
        use std::f32::consts::TAU;
        match self {
            Animation::Solid => 1.0,
            // 呼吸:周期约 2.4s,在 0.55..1.0 间起伏。
            Animation::Breathe => {
                let s = (phase * TAU / 2.4).sin() * 0.5 + 0.5;
                0.55 + 0.45 * s
            }
            // 闪烁:周期约 0.9s,在 0.3..1.0 间快速脉动。
            Animation::Blink => {
                let s = (phase * TAU / 0.9).sin() * 0.5 + 0.5;
                0.3 + 0.7 * s
            }
        }
    }
}

/// RGB 颜色(0..=255)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// 转为 Win32 `COLORREF`(0x00BBGGRR)。
    pub const fn colorref(self) -> u32 {
        (self.r as u32) | ((self.g as u32) << 8) | ((self.b as u32) << 16)
    }

    /// 各分量归一化到 0.0..=1.0,便于 Direct2D 使用。
    pub fn to_f32(self) -> (f32, f32, f32) {
        (
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
        )
    }
}

impl Status {
    /// 灯的基础颜色。
    pub const fn color(self) -> Rgb {
        match self {
            Status::Idle => Rgb::new(0x2E, 0xCC, 0x71),              // 绿
            Status::Thinking => Rgb::new(0x9B, 0x59, 0xB6),          // 紫
            Status::Working => Rgb::new(0x34, 0x98, 0xDB),           // 蓝
            Status::WaitingInput => Rgb::new(0xF1, 0xC4, 0x0F),      // 黄
            Status::WaitingPermission => Rgb::new(0xE6, 0x7E, 0x22), // 橙
            Status::Error => Rgb::new(0xE7, 0x4C, 0x3C),             // 红
            Status::Offline => Rgb::new(0x95, 0xA5, 0xA6),           // 灰
        }
    }

    /// 灯的动画方式。
    pub const fn animation(self) -> Animation {
        match self {
            Status::Idle | Status::Offline => Animation::Solid,
            Status::Thinking => Animation::Breathe,
            Status::Working => Animation::Breathe,
            Status::WaitingInput => Animation::Blink,
            Status::WaitingPermission => Animation::Blink,
            Status::Error => Animation::Solid,
        }
    }

    /// 是否需要用户介入(用于将来排序 / 强调)。
    pub const fn needs_attention(self) -> bool {
        matches!(
            self,
            Status::WaitingInput | Status::WaitingPermission | Status::Error
        )
    }

    /// 中文显示名(用于悬停提示)。
    pub const fn label_zh(self) -> &'static str {
        match self {
            Status::Idle => "空闲",
            Status::Thinking => "思考中",
            Status::Working => "工作中",
            Status::WaitingInput => "等待输入",
            Status::WaitingPermission => "等待授权",
            Status::Error => "出错",
            Status::Offline => "离线",
        }
    }
}

impl Default for Status {
    fn default() -> Self {
        Status::Idle
    }
}
