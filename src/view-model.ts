import { deriveTaskStatus, statusReason } from "./status";
import type {
  CmuxNotification,
  CmuxSourceState,
  CmuxWorkspace,
  MergedTask,
  TaskConfig,
} from "./types";

function fallbackConfig(workspace: CmuxWorkspace): TaskConfig {
  return {
    id: workspace.id.slice(0, 8),
    name: workspace.title || workspace.current_directory.split("/").pop() || "Unnamed",
    name_overridden: false,
    cmux_workspace_id: workspace.id,
    manual_status: null,
    note: "",
  };
}

export function taskDisplayName(workspace: CmuxWorkspace, config: TaskConfig): string {
  if (config.name_overridden && config.name.trim()) return config.name.trim();
  return workspace.title.trim()
    || config.name.trim()
    || workspace.current_directory.split("/").pop()
    || "Unnamed";
}

export function mergeWorkspaceTasks(
  workspaces: CmuxWorkspace[],
  notifications: CmuxNotification[],
  configs: TaskConfig[],
): MergedTask[] {
  return workspaces.map((workspace) => {
    const workspaceNotifications = notifications
      .filter((notification) => notification.workspace_id === workspace.id)
      .sort((a, b) => b.created_at.localeCompare(a.created_at));
    const config = configs.find((item) => item.cmux_workspace_id === workspace.id)
      || fallbackConfig(workspace);
    const input = {
      manualStatus: config.manual_status,
      notifications: workspaceNotifications,
      latestSubmittedAt: workspace.latest_submitted_at,
    };
    return {
      config,
      cmux: workspace,
      notifications: workspaceNotifications,
      hasUnread: workspaceNotifications.some((item) => !item.is_read),
      latestNotifSubtitle: workspaceNotifications[0]?.subtitle || null,
      effectiveStatus: deriveTaskStatus(input),
      ports: workspace.listening_ports || [],
      directory: workspace.current_directory,
      title: taskDisplayName(workspace, config),
      latestMessage: workspace.latest_conversation_message,
      statusReason: statusReason(input),
    };
  });
}

export function sourceMessage(source: CmuxSourceState): string {
  if (source.status === "ready") return "";
  switch (source.code) {
    case "CLI_NOT_FOUND":
      return "找不到 cmux CLI，请确认 cmux 已安装。";
    case "CMUX_NOT_RUNNING":
      return "cmux 未运行或 socket 不可用。";
    case "ACCESS_DENIED":
      return "cmux 拒绝外部连接，请将 automation.socketControlMode 设置为 allowAll 后重载或重启 cmux。";
    case "TIMEOUT":
      return "cmux 响应超时，Focus Bar 将继续重试。";
    case "INVALID_RESPONSE":
      return "cmux 返回了无法识别的数据。";
    case "WATCHER_DISCONNECTED":
      return "cmux 实时事件已断开，正在重连。";
  }
}
