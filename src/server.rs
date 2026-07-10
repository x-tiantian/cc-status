//! HTTP 监听服务(独立线程)。
//!
//! 两个端点(需求 §5.3):
//! - `POST /hook`   接收 Claude Code 原始 hook JSON,服务端映射状态
//! - `POST /status` 接收 cc-status 自定义精简协议
//!
//! 状态写入共享 Store 后,通过 `PostMessageW(WM_APP_UPDATE)` 唤醒 UI 线程刷新。

use crate::hook::{make_key, HookPayload};
use crate::state::{LightKey, SharedStore};
use crate::status::Status;
use crate::win::WM_APP_UPDATE;
use serde::Deserialize;
use std::sync::mpsc::Receiver;
use tiny_http::{Method, Response, Server};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// 自定义精简协议(`POST /status`),对应需求 §5.2。
#[derive(Debug, Deserialize)]
struct StatusPayload {
    project: String,
    state: Status,
    #[serde(default)]
    message: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    session: String,
}

/// 服务线程配置快照。IP/端口变更时由主线程重启线程。
pub struct ServerConfig {
    pub ip: String,
    pub port: u16,
    pub token: String,
}

/// 控制消息:目前仅用于优雅停止。
pub enum ServerCtrl {
    Stop,
}

/// 启动 HTTP 服务线程。返回值:线程句柄;绑定失败通过返回的 Result 反馈。
///
/// `hwnd` 用于回发刷新消息;`shutdown` 收到 Stop 后线程退出。
pub fn spawn(
    cfg: ServerConfig,
    store: SharedStore,
    hwnd_raw: isize,
    shutdown: Receiver<ServerCtrl>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let addr = format!("{}:{}", cfg.ip, cfg.port);
        let server = match Server::http(&addr) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[cc-status] 监听 {addr} 失败: {e}");
                return;
            }
        };
        eprintln!("[cc-status] 正在监听 http://{addr}");

        loop {
            // 非阻塞轮询,兼顾接收请求与响应停止信号。
            match server.recv_timeout(std::time::Duration::from_millis(200)) {
                Ok(Some(request)) => {
                    handle_request(request, &cfg, &store, hwnd_raw);
                }
                Ok(None) => {
                    if matches!(shutdown.try_recv(), Ok(ServerCtrl::Stop)) {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    })
}

fn handle_request(
    mut request: tiny_http::Request,
    cfg: &ServerConfig,
    store: &SharedStore,
    hwnd_raw: isize,
) {
    // 仅接受 POST。
    if *request.method() != Method::Post {
        let _ = request.respond(json_response(405, "{\"error\":\"method not allowed\"}"));
        return;
    }

    // token 校验(启用时)。
    if !cfg.token.is_empty() {
        let ok = request
            .headers()
            .iter()
            .any(|h| h.field.equiv("X-CC-Token") && h.value.as_str() == cfg.token);
        if !ok {
            let _ = request.respond(json_response(401, "{\"error\":\"unauthorized\"}"));
            return;
        }
    }

    let url = request.url().to_string();
    // 来源主机(用于跨机器区分同名项目)。
    let peer_host = request
        .remote_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_default();

    let mut body: Vec<u8> = Vec::new();
    if std::io::Read::read_to_end(request.as_reader(), &mut body).is_err() {
        let _ = request.respond(json_response(400, "{\"error\":\"bad body\"}"));
        return;
    }

    let outcome = if url.starts_with("/hook") {
        apply_hook(&body, &peer_host, store)
    } else if url.starts_with("/status") {
        apply_status(&body, &peer_host, store)
    } else {
        let _ = request.respond(json_response(404, "{\"error\":\"not found\"}"));
        return;
    };

    match outcome {
        Ok(changed) => {
            if changed {
                notify_ui(hwnd_raw);
            }
            // http hook 要求响应体为 JSON;返回空对象表示"已接收、不做决策"。
            let _ = request.respond(json_response(200, "{}"));
        }
        Err(msg) => {
            let body = format!("{{\"error\":\"{}\"}}", msg);
            let _ = request.respond(json_response(400, &body));
        }
    }
}

/// 构造带 `Content-Type: application/json` 头的响应。
fn json_response(code: u16, body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("valid header");
    Response::from_string(body)
        .with_status_code(code)
        .with_header(header)
}

/// 处理原始 hook JSON。返回 Ok(true) 表示状态有变。
fn apply_hook(body: &[u8], peer_host: &str, store: &SharedStore) -> Result<bool, &'static str> {
    let payload: HookPayload = serde_json::from_slice(body).map_err(|_| "invalid hook json")?;
    if std::env::var("CC_TRACE").is_ok() {
        eprintln!(
            "[trace] hook event={} type={} cwd={} session={}",
            payload.hook_event_name,
            payload.notification_type,
            payload.cwd,
            payload.session_id
        );
    }
    let update = match payload.to_update() {
        Some(u) => u,
        None => return Ok(false), // 不影响灯的事件
    };
    // hook JSON 不含主机名,用连接来源 IP;回环视为本机(空 host)。
    let host = host_label(peer_host);
    let key = make_key(&host, &update.key_project);

    let mut s = store.lock().map_err(|_| "store poisoned")?;
    if update.remove {
        // SessionEnd:标记 offline(实际移除交给超时清理,避免瞬时闪断)。
        s.upsert(key, Status::Offline, update.message);
    } else {
        s.upsert(key, update.status, update.message);
    }
    Ok(true)
}

/// 处理自定义精简协议。
fn apply_status(body: &[u8], peer_host: &str, store: &SharedStore) -> Result<bool, &'static str> {
    let p: StatusPayload = serde_json::from_slice(body).map_err(|_| "invalid status json")?;
    if p.project.is_empty() {
        return Err("missing project");
    }
    let host = if p.host.is_empty() {
        host_label(peer_host)
    } else {
        p.host
    };
    let key = LightKey::new(host, p.project);
    let mut s = store.lock().map_err(|_| "store poisoned")?;
    s.upsert(key, p.state, p.message);
    Ok(true)
}

/// 回环来源视为本机(空 host 标签),其余用 IP。
fn host_label(peer_host: &str) -> String {
    if peer_host.is_empty() || peer_host == "127.0.0.1" || peer_host == "::1" {
        String::new()
    } else {
        peer_host.to_string()
    }
}

/// 通知 UI 线程刷新。
fn notify_ui(hwnd_raw: isize) {
    if hwnd_raw == 0 {
        return;
    }
    unsafe {
        let _ = PostMessageW(
            Some(HWND(hwnd_raw as *mut _)),
            WM_APP_UPDATE,
            WPARAM(0),
            LPARAM(0),
        );
    }
}
