//! 项目状态存储:线程安全的灯集合。
//!
//! HTTP 线程写入(收到推送),UI 线程读取(绘制)。以 `(host, project, session)`
//! 为唯一键区分不同的灯(对应需求文档 §5.2)。心跳/超时逻辑在 M2 完善。

use crate::status::Status;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 一个灯的唯一标识:以「主机 + 项目」区分。
///
/// 注意:**不含 session**——同一项目多开会话应合并为一个灯(用户关心的是
/// 哪个项目需要介入,而非哪个会话)。跨机器同名项目靠 host 区分。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LightKey {
    pub host: String,
    pub project: String,
}

impl LightKey {
    pub fn new(host: impl Into<String>, project: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            project: project.into(),
        }
    }
}

/// 单个项目的当前状态。
#[derive(Debug, Clone)]
pub struct Light {
    pub status: Status,
    /// Claude 给到的消息(悬停显示)。
    pub message: String,
    /// 最后一次收到推送的时刻(心跳)。
    pub last_beat: Instant,
}

impl Light {
    pub fn new(status: Status, message: String) -> Self {
        Self {
            status,
            message,
            last_beat: Instant::now(),
        }
    }
}

/// 全部灯的集合。用 BTreeMap 保证展示顺序稳定。
#[derive(Debug, Default)]
pub struct Store {
    lights: BTreeMap<LightKey, Light>,
}

impl Store {
    /// 插入或更新一个灯,并刷新心跳。
    ///
    /// 消息随状态一同更新:新消息为空则清空旧消息,避免过时消息与新状态不符
    /// (例如状态已变为"工作中",却仍显示上一条"等待输入"的消息)。
    pub fn upsert(&mut self, key: LightKey, status: Status, message: String) {
        self.lights
            .entry(key)
            .and_modify(|l| {
                l.status = status;
                l.message = message.clone();
                l.last_beat = Instant::now();
            })
            .or_insert_with(|| Light::new(status, message));
    }

    /// 当前灯数量。
    pub fn len(&self) -> usize {
        self.lights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lights.is_empty()
    }

    /// 按稳定顺序返回灯的快照(供 UI 绘制)。
    pub fn snapshot(&self) -> Vec<(LightKey, Light)> {
        self.lights
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// 心跳清理:超过 `offline_after` 无心跳 → 置 offline;
    /// 超过 `remove_after` → 移除。返回 true 表示集合有变化(需刷新 UI)。
    pub fn sweep(&mut self, offline_after: Duration, remove_after: Duration) -> bool {
        let now = Instant::now();
        let mut changed = false;

        // 先移除超过 remove_after 的灯。
        let before = self.lights.len();
        self.lights
            .retain(|_, l| now.duration_since(l.last_beat) < remove_after);
        if self.lights.len() != before {
            changed = true;
        }

        // 再把超时的灯置为 offline。
        for l in self.lights.values_mut() {
            if l.status != Status::Offline && now.duration_since(l.last_beat) >= offline_after {
                l.status = Status::Offline;
                changed = true;
            }
        }
        changed
    }
}

/// 线程间共享的存储句柄。
pub type SharedStore = Arc<Mutex<Store>>;

/// 新建一个共享存储。
pub fn new_shared() -> SharedStore {
    Arc::new(Mutex::new(Store::default()))
}
