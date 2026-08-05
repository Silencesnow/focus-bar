# Focus Bar 每日活动耗时统计设计

## 目标

Focus Bar 在本机统计用户每天围绕 AI 任务投入的有效时间，并细分到具体 Focus Bar 任务。统计覆盖：

- cmux 中给 AI 输入、阅读 AI 输出；
- Codex 中给 AI 输入、阅读 AI 输出；
- Chrome 中 Review 与任务关联的页面；
- VS Code 中阅读代码、编辑代码。

统计用于复盘个人时间分配，不用于监控键盘内容、构建完整浏览历史或衡量 AI 后台运行时长。

## 已确认的产品规则

- 90 秒没有键盘、鼠标移动、点击或滚动后停止累计有效时间。
- 统计必须归属到具体 Focus Bar 任务，同时提供 cmux、Codex、Chrome、VS Code 的来源汇总。
- 无法可靠确定任务时进入“未归属”，不得猜测到最近活跃任务。
- 统计入口位于 Focus Bar 底部，点击后打开独立统计窗口。
- 统计窗口默认展示今天，并支持最近 7 天。
- 所有数据只保存在本机。

## 方案选择

采用原生混合采集：macOS 前台应用与操作事件负责计时，Focus Bar 已有的 cmux workspace、Codex thread、Chrome URL 和 VS Code workspace 映射负责任务归属。

不采用以下方案：

- 仅按前台应用计时：无法满足任务级归属。
- 第一版即开发 Chrome、VS Code 扩展：安装和维护成本过高；只有原生采集验证出明确精度缺口后才考虑。

## 活动分类

### cmux

当 cmux 是前台应用时，读取当前 window、workspace 和 selected terminal surface：

- selected surface 对应 Claude Code 会话，且可编辑输入区域获得焦点并发生键盘输入：`ai_input`；
- selected surface 对应 Claude Code 会话，没有输入但存在鼠标、点击、滚动或未超过空闲阈值：`ai_reading`；
- 普通 Shell、开发服务器或非 AI surface 不计入 AI 时间。

任务通过 `cmux_workspace_id` 关联。

### Codex

当 Codex 是前台应用时：

- Codex 输入区域获得焦点并发生键盘输入：`ai_input`；
- 没有输入但存在鼠标、点击、滚动或未超过空闲阈值：`ai_reading`。

任务归属按以下优先级决定：

1. 辅助功能能够识别的当前选中 Codex 任务；
2. 当前 Codex 窗口仍对应最近一次由 Focus Bar 成功跳转的 `thread_id`；
3. 无法确定时记入“未归属”。

Codex 后台执行、subagent 或 shell 的运行时间不计入用户投入时间。Codex app-server 和本地状态仅用于获取 thread 身份与运行状态，不以后台更新时间推断用户正在阅读哪个任务。

### Chrome

当普通 Chrome 是前台应用时读取当前活动 Tab：

- URL 与某个任务配置的 Chrome 链接按现有前缀规则匹配：`browser_review`；
- 不匹配任何任务：不记录 URL，也不计入任务耗时。

任务通过匹配到的 Chrome target 关联。数据库只保存任务 ID 和 target 索引，不保存完整 URL。

### VS Code

当 VS Code 是前台应用时，通过窗口标题和 workspace 目录匹配任务配置：

- 可编辑代码区域发生键盘输入：`code_editing`；
- 没有编辑输入但存在鼠标、点击、滚动或未超过空闲阈值：`code_reading`；
- 搜索框、命令面板等非代码编辑区域的输入记为 `code_reading`，不记为编辑代码。

无法匹配 workspace 时进入“未归属”。

## 采集架构

### Native Activity Collector

在 Tauri/Rust 后端新增常驻采集器：

- 监听前台应用变化；
- 监听键盘、鼠标点击、移动和滚动的时间戳，不记录按键内容；
- 每秒执行轻量计时 tick；
- 前台应用变化时立即解析上下文；cmux、Chrome、Codex、VS Code 保持前台期间每 2 秒检查一次轻量上下文键，以识别同一应用内的 workspace、Tab、thread 或窗口切换；
- 只有轻量上下文键变化时才执行完整的 cmux、Chrome、VS Code 或辅助功能查询；
- 90 秒无操作时结束当前时间段。

采集器输出统一的 `ObservedActivity`：

```text
source              cmux | codex | chrome | vscode
activity_type       ai_input | ai_reading | browser_review | code_reading | code_editing
task_id             Focus Bar task id | null
confidence          high | medium | low
observed_at         timestamp
```

### Context Resolvers

每个来源有独立 resolver，只负责把当前应用状态转换为任务和活动类型：

- `CmuxContextResolver`
- `CodexContextResolver`
- `ChromeContextResolver`
- `VscodeContextResolver`

resolver 查询失败不得停止采集器。该 tick 进入未归属或跳过，并记录精简错误状态供统计窗口提示。

### Segment Aggregator

聚合器把连续 tick 合并为时间段：

- 来源、任务、活动类型或置信度变化时结束旧段；
- 前后台应用切换时立即结束旧段；
- 超过 90 秒无操作时，将段的结束时间截断到最后一次有效操作后 90 秒；
- 小于 2 秒的瞬时切换不落库，避免应用切换噪声；
- App 正常退出时刷新当前段；异常退出时通过定期 checkpoint 将最大损失限制在 10 秒内。

## 数据存储

新增独立本地 SQLite 数据库，避免把高频统计写入 `~/.focus.json`。

核心表：

```sql
activity_segments(
  id INTEGER PRIMARY KEY,
  started_at INTEGER NOT NULL,
  ended_at INTEGER NOT NULL,
  source TEXT NOT NULL,
  activity_type TEXT NOT NULL,
  task_id TEXT,
  task_title TEXT,
  confidence TEXT NOT NULL,
  context_key TEXT
)
```

`context_key` 只保存 workspace ID、thread ID、Chrome target 索引或 workspace 规范化标识，不保存输入内容、页面内容和未匹配 URL。

任务被关闭或配置消失后，历史记录仍以任务 ID 保留；展示时优先使用当前任务名，否则使用时间段创建时保存的短标题快照。

## 统计窗口

Focus Bar 底部新增统计按钮，打开独立窗口。窗口包含：

- 今天 / 最近 7 天切换；
- 总有效时间；
- 按任务汇总，可展开查看来源和活动类型；
- 按来源汇总；
- “未归属”和低置信度区块；
- 权限缺失或来源解析失败提示。

第一版使用时长条和文本数据，不加入目标管理、打卡、导出或生产力评分。

## 权限与隐私

- 辅助功能：识别前台窗口、焦点元素、Codex 与 VS Code 上下文；
- 输入监控：只读取“发生了键盘/鼠标事件”和时间戳，不读取或保存按键值；
- 自动化：仅在 Chrome 位于前台时查询活动 Tab；
- cmux socket：沿用现有访问方式。

统计窗口必须显示每项权限状态。权限被拒绝时，受影响来源停止精细分类，不静默生成误导性数据。

## 准确度与边界

- “阅读”是前台应用、当前任务和近期操作共同推断的有效关注时间，不是眼动追踪。
- 用户静止阅读超过 90 秒时，超过部分不计入；这是已确认的防止离开电脑误记规则。
- Codex 当前 thread 无法可靠识别时进入未归属，不使用后台更新时间猜测。
- Chrome 仅统计配置过的任务链接，不统计普通网页浏览。
- VS Code 编辑分类依赖焦点处于代码编辑区域；无法识别焦点时降级为阅读或未归属，并降低置信度。

## 错误处理

- resolver 超时上限为 1 秒，失败不阻塞其他来源；
- 连续失败采用退避，前台应用变化时立即重试；
- 数据库写入失败时保留有限内存队列，并在统计窗口提示；
- 系统休眠、锁屏、切换用户时立即结束当前时间段；唤醒后从新操作重新开始。

## 测试与验收

### 单元测试

- 90 秒空闲截断；
- 连续 tick 合并与上下文切换分段；
- cmux workspace、Codex thread、Chrome URL、VS Code workspace 任务归属；
- 输入与阅读分类；
- 未归属、低置信度和权限降级；
- 跨午夜和最近 7 天聚合。

### 集成测试

- SQLite 写入、checkpoint、重启恢复；
- 权限拒绝与 resolver 超时；
- Focus Bar 跳转 Codex 后的 thread 归属；
- Chrome MR 根页、files、commits 使用同一任务归属。

### 手工验收

分别在 cmux、Codex、Chrome、VS Code 中完成短时输入/阅读流程，确认：

- 来源和任务正确；
- 90 秒空闲后停止增长；
- 不匹配的网页不被记录；
- 统计窗口当天汇总与原始时间段一致；
- 应用切换、休眠和重启不产生重叠或超长时间段。

## 第一版范围外

- Chrome 或 VS Code 扩展；
- 眼动或摄像头监测；
- 输入内容、剪贴板内容、页面正文采集；
- 云同步、团队报表、生产力评分；
- CSV 导出和自定义统计规则。
