# Focus Bar

Focus Bar 是一个 macOS 本地注意力提示条。当前 MVP 只连接 cmux：显示正在运行的 workspace、根据通知推断注意力状态，并在点击后跳转到正确的 cmux window/workspace。

## 当前状态

- `🔴 需要处理`：cmux 有未读 Waiting、输入请求、阻塞或失败通知。
- `🟡 待检查`：cmux 有未读 Completed、Done 或成功通知。
- `🟢 执行中`：最近一次提交晚于最近一次终态通知。
- `⬜ 空闲`：没有需要注意的活动。

右键任务可以临时覆盖状态；选择“自动判断”会恢复自动状态。界面只显示当前打开的 cmux workspace，`~/.focus.json` 中的历史记录不会被删除。

## Chrome 与 VS Code 跳转配置

点击提示条右上角的 `⚙️`，或右键任务选择“配置跳转目标”。配置窗口会列出当前 cmux workspace：

- Chrome 完整 URL：优先在所有 Chrome 窗口中精确匹配已有 tab；找不到时打开新 tab。
- VS Code workspace 名称：用于匹配当前已打开窗口，可留空并从目录名推断。
- VS Code workspace 目录：必填绝对路径。
- 文件与行号：可选，文件路径相对于 workspace。

配置窗口可以在保存前分别测试 Chrome 和 VS Code。保存后，提示条会立即出现 `🌐` 和 `📝` 图标，不需要等待轮询。

首次使用时 macOS 可能请求权限：

- Chrome 需要“系统设置 → 隐私与安全性 → 自动化”中的 Google Chrome 控制权限。
- VS Code 已打开窗口的精确聚焦需要“系统设置 → 隐私与安全性 → 辅助功能”权限。

Focus Bar 只把 URL 和路径作为独立进程参数传递，不会拼接到 AppleScript 源码或 shell 命令中。

## 前置条件

1. macOS 14 或更高版本。
2. 已安装 `/Applications/cmux.app`。
3. cmux 允许同一用户下的外部本地进程访问 socket。

在 cmux 配置 `~/.config/cmux/cmux.json` 中设置：

```json
{
  "$schema": "https://raw.githubusercontent.com/manaflow-ai/cmux/main/web/data/cmux.schema.json",
  "schemaVersion": 1,
  "automation": {
    "socketControlMode": "allowAll"
  }
}
```

修改前先备份配置。修改后按当前 cmux 版本的要求 reload 或重启。Focus Bar 只检查这个条件，不会自动修改 cmux 设置。

## 开发

```bash
bun install
bun run tauri dev
```

Focus Bar 按以下顺序查找 CLI：

1. `CMUX_BUNDLED_CLI_PATH`
2. 当前 `PATH` 中的 `cmux`
3. `/Applications/cmux.app/Contents/Resources/bin/cmux`
4. `~/Applications/cmux.app/Contents/Resources/bin/cmux`

socket 优先使用 `CMUX_SOCKET_PATH`，否则读取 `~/.local/state/cmux/last-socket-path`，最后交给 cmux CLI 自动发现。

## 验证

```bash
bun run check
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
python3 scripts/test_cmux_live.py
```

live doctor 默认只读。只有显式传入下面的参数时才会切换 workspace：

```bash
python3 scripts/test_cmux_live.py --jump workspace:1
```

## 数据源错误

- `CLI_NOT_FOUND`：找不到 cmux CLI。
- `CMUX_NOT_RUNNING`：cmux 未运行或 socket 不存在。
- `ACCESS_DENIED`：通常表示 `socketControlMode` 仍是 `cmuxOnly`。
- `TIMEOUT`：cmux 在限定时间内没有响应。
- `INVALID_RESPONSE`：cmux 返回的数据无法解析。
- `WATCHER_DISCONNECTED`：实时事件流断开；界面保留最后一次成功数据，并继续重连和轮询。
