import { deriveTaskStatus, runningSurfaceSummary, statusReason } from "./status";
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

function latestValidTimestamp(values: Array<string | null | undefined>): string | null {
  let latest: { value: string; timestamp: number } | null = null;
  for (const value of values) {
    if (!value) continue;
    const timestamp = Date.parse(value);
    if (!Number.isFinite(timestamp)) continue;
    if (!latest || timestamp > latest.timestamp) latest = { value, timestamp };
  }
  return latest?.value || null;
}

function activitySummary(
  workspace: CmuxWorkspace,
  notifications: CmuxNotification[],
  reason: string | null,
  surfaceSummary: string | null,
): string | null {
  const message = workspace.latest_conversation_message?.trim();
  if (message) return message;
  const progress = workspace.active_surface_progress?.trim();
  if (progress) return progress;
  if (surfaceSummary) return surfaceSummary;
  const notification = notifications[0];
  return notification?.subtitle.trim()
    || notification?.body.trim()
    || notification?.title.trim()
    || reason?.trim()
    || null;
}

export function formatRelativeTime(value: string | null, now = Date.now()): string | null {
  if (!value) return null;
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) return null;
  const elapsed = Math.max(0, now - timestamp);
  const minutes = Math.floor(elapsed / 60_000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes}分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}小时前`;
  return `${Math.floor(hours / 24)}天前`;
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
      activeSurfaceTitle: workspace.active_surface_title,
    };
    const reason = statusReason(input);
    const surfaceSummary = runningSurfaceSummary(workspace.active_surface_title);
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
      statusReason: reason,
      activitySummary: activitySummary(
        workspace,
        workspaceNotifications,
        reason,
        surfaceSummary,
      ),
      activityAt: surfaceSummary && !workspace.latest_submitted_at
        ? null
        : latestValidTimestamp([
            workspace.latest_submitted_at,
            ...workspaceNotifications.map((notification) => notification.created_at),
          ]),
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
