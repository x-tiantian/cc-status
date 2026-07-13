//! 解析 Claude Code 原始 hook JSON,并映射为 cc-status 的灯键与状态。
//!
//! 对应需求文档 §5.3。字段与映射依据官方 hooks 文档
//! (https://code.claude.com/docs/en/hooks)。

use crate::state::LightKey;
use crate::status::Status;
use serde::Deserialize;

/// Claude Code hook 事件的共有 + 相关字段(未知字段忽略)。
#[derive(Debug, Deserialize)]
pub struct HookPayload {
    #[serde(default)]
    pub hook_event_name: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
    /// Notification 事件的子类型:permission_prompt / idle_prompt / ...
    #[serde(default)]
    pub notification_type: String,
    /// 人类可读消息(Notification 等携带)。
    #[serde(default)]
    pub message: String,
    /// Notification 的标题。
    #[serde(default)]
    pub title: String,
    /// 工具名(PreToolUse / Notification permission 携带)。
    #[serde(default)]
    pub tool_name: String,
}

/// hook 解析结果:一次状态更新。`None` 表示该事件不产生状态变更。
pub struct HookUpdate {
    pub key_project: String,
    pub status: Status,
    pub message: String,
    /// SessionEnd 表示会话结束,应移除该灯。
    pub remove: bool,
}

impl HookPayload {
    /// 从 cwd 提取项目名(末级目录名);为空时回退 "unknown"。
    fn project_name(&self) -> String {
        let cwd = self.cwd.replace('\\', "/");
        cwd.trim_end_matches('/')
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("unknown")
            .to_string()
    }

    /// 依据事件名 + 子类型映射为状态更新。
    pub fn to_update(&self) -> Option<HookUpdate> {
        let (status, remove) = match self.hook_event_name.as_str() {
            "SessionStart" => (Status::Idle, false),
            // 收到输入 / 消化工具结果 → 推理阶段(近似 thinking)。
            "UserPromptSubmit" | "PostToolUse" => (Status::Thinking, false),
            // 即将执行工具 → 工作中。
            "PreToolUse" => (Status::Working, false),
            "Notification" => match self.notification_type.as_str() {
                "permission_prompt" => (Status::WaitingPermission, false),
                "idle_prompt" => (Status::WaitingInput, false),
                _ => return None, // auth_success 等不影响灯
            },
            "PermissionRequest" => (Status::WaitingPermission, false),
            "Stop" => (Status::Idle, false),
            "StopFailure" => (Status::Error, false),
            "SessionEnd" => (Status::Offline, true),
            _ => return None,
        };

        Some(HookUpdate {
            key_project: self.project_name(),
            status,
            message: self.display_message(status),
            remove,
        })
    }

    /// 组装悬停显示的消息。优先用 message,其次用 title/tool_name 兜底。
    fn display_message(&self, status: Status) -> String {
        if !self.message.is_empty() {
            return self.message.clone();
        }
        match status {
            Status::WaitingPermission if !self.tool_name.is_empty() => {
                format!("请求授权:{}", self.tool_name)
            }
            _ if !self.title.is_empty() => self.title.clone(),
            _ => String::new(),
        }
    }
}

/// 构造灯键。host 由服务端从连接来源补充。session 不参与去重(同项目多会话合并)。
pub fn make_key(host: &str, project: &str) -> LightKey {
    LightKey::new(host, project)
}
