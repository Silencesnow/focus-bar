# Focus Bar

Focus Bar 是一个 macOS 本地注意力提示条。当前 MVP 只连接 cmux：显示正在运行的 workspace、根据通知推断注意力状态，并在点击后跳转到正确的 cmux window/workspace。

## 当前状态

- `🔴 需要处理`：cmux 有未读 Waiting、输入请求、阻塞或失败通知。
- `🟡 待检查`：cmux 有未读 Completed、Done 或成功通知。
- `🟢 执行中`：最近一次提交晚于最近一次终态通知。
- `⬜ 空闲`：没有需要注意的活动。

右键任务可以临时覆盖状态；选择“自动判断”会恢复自动状态。界面只显示当前打开的 cmux workspace，`~/.focus.json` 中的历史记录不会被删除。

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
