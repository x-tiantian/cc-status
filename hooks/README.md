# Claude Code Hooks 配置说明 / Hook Setup

把 `settings.example.json` 里的 `hooks` 键合并到你的 Claude Code `settings.json`
(通常在 `~/.claude/settings.json`,Windows 为 `C:\Users\<你>\.claude\settings.json`),
与已有的 `env`、`model` 等键平级。

Merge the `hooks` key from `settings.example.json` into your Claude Code
`settings.json` (usually `~/.claude/settings.json`), alongside existing keys.

配置后**重启 Claude Code** 生效(hook 仅在启动时加载)。
Restart Claude Code afterwards — hooks are only loaded at startup.

## 快速获取 / Quick generate

cc-status 可直接打印当前配置对应的片段:
cc-status can print the snippet for your current IP/port:

```
cc-status.exe --print-hooks
```

设置窗里也会动态显示并提供"复制"按钮。
The settings window also shows it live with a Copy button.

## 跨机器 / Cross-machine

把 URL 里的 `127.0.0.1` 换成 cc-status 所在机器的局域网 IP,例如
`http://192.168.1.20:9898/hook`;并建议启用 token(见下)。

Replace `127.0.0.1` with the LAN IP of the machine running cc-status, e.g.
`http://192.168.1.20:9898/hook`, and enable a token (below).

## 启用 Token / With a token

若在 cc-status 设置里配置了 token,请给每个 hook 加上请求头:
If you set a token in cc-status settings, add the header to each hook:

```json
{ "type": "http", "url": "http://127.0.0.1:9898/hook", "timeout": 5,
  "headers": { "X-CC-Token": "your-token" } }
```
