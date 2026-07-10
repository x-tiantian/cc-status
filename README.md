# cc-status

> Windows 任务栏托盘旁的 Claude CLI 状态监控 · A taskbar tray status monitor for Claude CLI

**语言 / Language:** [中文](#中文) · [English](#english)

同时运行多个 Claude CLI(Claude Code)时,cc-status 在系统托盘左侧用"红绿灯"实时显示每个项目的工作状态,让你无需盯着终端,也能第一时间发现**需要对话或授权**的时机。

When you run several Claude CLI (Claude Code) sessions at once, cc-status shows a row of "traffic lights" next to the system tray — one per project — so you can tell at a glance which session **needs your input or approval**, without watching the terminals.

原生 Rust + Win32 实现,**无 Electron / 无浏览器内核**,免安装单 exe。
Native Rust + Win32. **No Electron, no browser engine.** A single portable exe.

---

## 中文

### ✨ 特性

- 🚦 **红绿灯状态**:每个项目一盏灯,颜色随状态实时切换,需要介入时闪烁提醒
- 🖥️ **贴合托盘**:锚定在系统托盘图标左侧;定位失败自动降级到屏幕右下角
- 🔢 **多项目 + 分屏**:最多同屏 3 盏,超出则每 3 个一屏、定时上下翻页轮播
- 💬 **悬停详情**:鼠标移到灯上显示项目名、状态与 Claude 的消息
- ⚙️ **右键设置**:改监听 IP/端口、开机自启,并动态生成 hook 配置供一键复制
- 🌐 **跨机器**:监听局域网地址即可集中显示多台机器上的会话状态
- 🪶 **轻量**:空闲时 CPU ≈ 0,内存 < 30MB;原生 Win32,免安装

### 🖼️ 状态与配色

| 状态 | 含义 | 灯 | 动画 | 需要你介入 |
|------|------|----|------|:---:|
| `idle` | 空闲 / 完成待命 | 🟢 绿 | 常亮 | |
| `working` | 思考 / 执行中 | 🔵 蓝 | 呼吸 | |
| `waiting_input` | 等待输入 / 对话 | 🟡 黄 | 闪烁 | ✅ |
| `waiting_permission` | 等待授权 | 🟠 橙 | 闪烁 | ✅ |
| `error` | 出错 | 🔴 红 | 常亮 | ✅ |
| `offline` | 超时无心跳 | ⚪ 灰 | 常亮 | |

### 🚀 快速开始

1. **下载 / 构建** `cc-status.exe`(见[从源码构建](#-从源码构建)),双击运行。托盘出现图标即已就绪。
2. **配置 Claude Code hooks**:把 [`hooks/settings.example.json`](hooks/settings.example.json) 里的 `hooks` 键合并进你的 `~/.claude/settings.json`,然后**重启 Claude Code**。
   - 也可运行 `cc-status.exe --print-hooks` 直接打印当前配置对应的片段。
3. 在任意项目里使用 Claude Code —— 对应的灯就会出现并随状态变化。

> 环境要求:Windows 10 1809+ / Windows 11,x86-64。免安装,普通用户权限即可。

### 🔌 工作原理

Claude Code 支持 `http` 类型的 hook,会在会话事件发生时把事件 JSON **直接 POST** 到指定 URL(无需脚本,失败也不阻塞 Claude)。cc-status 监听 `POST /hook`,按事件映射为灯的状态:

| Claude Code 事件 | → 状态 |
|---|---|
| `SessionStart` | 🟢 idle |
| `UserPromptSubmit` / `PreToolUse` | 🔵 working |
| `Notification`(permission_prompt) | 🟠 waiting_permission |
| `Notification`(idle_prompt) | 🟡 waiting_input |
| `Stop` | 🟢 idle |
| `StopFailure` | 🔴 error |
| `SessionEnd` | ⚪ offline → 移除 |

项目名取自事件里的工作目录(`cwd`);**同一项目的多个会话合并为一盏灯**。超过约 60 秒无推送转为灰色,再持续更久则移除该灯。

### ⚙️ 设置

右键任意一盏灯(或托盘图标)→ **设置**:

- **监听 IP / 端口**:默认 `127.0.0.1:9898`
- **开机启动**:默认关闭(写入 `HKCU\...\Run`,用户级、无需管理员)
- **Token**(可选):填写后,推送需携带 `X-CC-Token` 请求头,否则拒绝
- **hook 配置**:根据当前 IP/端口动态生成,点"复制"即可粘贴到 Claude Code

配置保存于 `%APPDATA%\cc-status\config.json`。

### 🌐 跨机器监控

把监听 IP 设为 `0.0.0.0` 或本机某网卡的局域网 IP;在**其他机器**的 Claude Code hook 里,把 URL 指向 cc-status 所在机器,例如 `http://192.168.1.20:9898/hook`。这样一台机器就能集中显示多台机器上所有会话的状态。

> ⚠️ 监听非回环地址会对局域网开放该端口,**强烈建议设置 Token**。

### 🛠️ 命令行

```
cc-status.exe                    # 正常启动(托盘常驻)
cc-status.exe --print-hooks      # 打印当前配置对应的 hooks 片段
cc-status.exe --enable-autostart # 启用开机自启
cc-status.exe --disable-autostart# 关闭开机自启
```

### 📡 自定义上报(可选)

除了原生 hook,也可直接向 `POST /status` 发送精简 JSON,便于非 Claude 的自定义集成:

```jsonc
POST http://127.0.0.1:9898/status
{
  "project": "my-repo",        // 必填,灯的标识
  "state":   "waiting_input",  // 必填,见上表状态枚举
  "message": "需要确认…",       // 可选,悬停显示
  "host":    "DEV-PC",         // 可选,跨机器区分同名项目
  "ts":      1752000000        // 可选,Unix 秒
}
```

### 📦 从源码构建

需要 Rust(1.85+,edition 2024)与 MSVC 工具链:

```bash
rustup target add x86_64-pc-windows-msvc
cargo build --release
# 产物:target/release/cc-status.exe
```

> 若企业网络导致 `cargo` 证书吊销检查失败(`CRYPT_E_REVOCATION_OFFLINE`),在项目根目录新建 `.cargo/config.toml` 并加入:
> ```toml
> [http]
> check-revoke = false
> ```

### 📄 许可证

[MIT](LICENSE)

---

## English

### ✨ Features

- 🚦 **Traffic-light status** — one light per project, color follows state, blinks when it needs you
- 🖥️ **Docked to the tray** — anchored just left of the notification area; falls back to the bottom-right corner if anchoring fails
- 🔢 **Multi-project + paging** — up to 3 lights at once; more are shown 3-per-page, cycling on a timer
- 💬 **Hover for details** — project name, state, and Claude's message
- ⚙️ **Right-click settings** — change listen IP/port, autostart, and copy a ready-made hook config
- 🌐 **Cross-machine** — listen on a LAN address to monitor sessions from several machines in one place
- 🪶 **Lightweight** — ~0% CPU idle, < 30MB RAM; native Win32, no install

### 🖼️ States & colors

| State | Meaning | Light | Animation | Needs you |
|-------|---------|-------|-----------|:---:|
| `idle` | Idle / done | 🟢 green | steady | |
| `working` | Thinking / running | 🔵 blue | breathe | |
| `waiting_input` | Awaiting your input | 🟡 yellow | blink | ✅ |
| `waiting_permission` | Awaiting approval | 🟠 orange | blink | ✅ |
| `error` | Error | 🔴 red | steady | ✅ |
| `offline` | Heartbeat timed out | ⚪ grey | steady | |

### 🚀 Quick start

1. **Download / build** `cc-status.exe` (see [Building](#-building-from-source)) and run it. A tray icon means it's ready.
2. **Configure Claude Code hooks**: merge the `hooks` key from [`hooks/settings.example.json`](hooks/settings.example.json) into your `~/.claude/settings.json`, then **restart Claude Code**.
   - Or run `cc-status.exe --print-hooks` to print the snippet for your current config.
3. Use Claude Code in any project — its light appears and tracks the session state.

> Requirements: Windows 10 1809+ / Windows 11, x86-64. Portable, no admin rights needed.

### 🔌 How it works

Claude Code supports `http` hooks that **POST the event JSON directly** to a URL (no scripts; failures never block Claude). cc-status listens on `POST /hook` and maps events to light states:

| Claude Code event | → state |
|---|---|
| `SessionStart` | 🟢 idle |
| `UserPromptSubmit` / `PreToolUse` | 🔵 working |
| `Notification` (permission_prompt) | 🟠 waiting_permission |
| `Notification` (idle_prompt) | 🟡 waiting_input |
| `Stop` | 🟢 idle |
| `StopFailure` | 🔴 error |
| `SessionEnd` | ⚪ offline → removed |

The project name comes from the event's working directory (`cwd`); **multiple sessions of the same project share one light**. No push for ~60s turns the light grey; longer removes it.

### ⚙️ Settings

Right-click any light (or the tray icon) → **Settings**:

- **Listen IP / port** — default `127.0.0.1:9898`
- **Start on boot** — off by default (writes `HKCU\...\Run`, per-user, no admin)
- **Token** (optional) — when set, pushes must carry the `X-CC-Token` header
- **Hook config** — generated live from your IP/port, with a Copy button

Config lives in `%APPDATA%\cc-status\config.json`.

### 🌐 Cross-machine monitoring

Set the listen IP to `0.0.0.0` or a LAN address, and point the Claude Code hooks on **other machines** at this host, e.g. `http://192.168.1.20:9898/hook`. One machine then shows every session across your fleet.

> ⚠️ Listening on a non-loopback address exposes the port on your LAN — **set a Token**.

### 🛠️ Command line

```
cc-status.exe                     # run (stays in the tray)
cc-status.exe --print-hooks       # print the hooks snippet for the current config
cc-status.exe --enable-autostart  # enable start-on-boot
cc-status.exe --disable-autostart # disable start-on-boot
```

### 📡 Custom reporting (optional)

Besides native hooks, you can POST a compact JSON to `POST /status` for non-Claude integrations:

```jsonc
POST http://127.0.0.1:9898/status
{
  "project": "my-repo",        // required, the light's id
  "state":   "waiting_input",  // required, see the state table above
  "message": "please confirm", // optional, shown on hover
  "host":    "DEV-PC",         // optional, disambiguates same-named projects
  "ts":      1752000000        // optional, Unix seconds
}
```

### 📦 Building from source

Requires Rust (1.85+, edition 2024) and the MSVC toolchain:

```bash
rustup target add x86_64-pc-windows-msvc
cargo build --release
# output: target/release/cc-status.exe
```

> If a corporate network breaks `cargo`'s cert-revocation check (`CRYPT_E_REVOCATION_OFFLINE`), create `.cargo/config.toml` in the project root with:
> ```toml
> [http]
> check-revoke = false
> ```

### 📄 License

[MIT](LICENSE)
