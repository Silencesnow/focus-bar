import type { CmuxNotification } from "./types";

export type AttentionStatus = "needs_action" | "needs_review" | "executing" | "idle";

export interface StatusInput {
  manualStatus: unknown;
  notifications: CmuxNotification[];
  latestSubmittedAt: string | null;
  activeSurfaceTitle?: string | null;
  agentLifecycle?: string | null;
  backgroundShellProcess?: string | null;
  agentEventKind?: "question" | "stop" | "running" | null;
  agentEventAt?: string | null;
  lastViewedAt?: string | null;
}

const ACTION_PATTERN = /\b(input required|needs? input|blocked|error|failed|failure)\b/i;
const REVIEW_PATTERN = /\b(completed|done|success|succeeded|finished)\b/i;
const RUNNING_SURFACE_PATTERN = /^[\u2800-\u28ff](?:\s+|$)/u;
const WAITING_SURFACE_PATTERN = /^✳(?:\s+|$)/u;

export function runningSurfaceSummary(value: string | null | undefined): string | null {
  if (!value || !RUNNING_SURFACE_PATTERN.test(value)) return null;
  return value.replace(RUNNING_SURFACE_PATTERN, "").trim() || null;
}

export function waitingSurfaceSummary(value: string | null | undefined): string | null {
  if (!value || !WAITING_SURFACE_PATTERN.test(value)) return null;
  return value.replace(WAITING_SURFACE_PATTERN, "").trim() || null;
}

function notificationText(notification: CmuxNotification): string {
  return [notification.title, notification.subtitle, notification.body]
    .filter(Boolean)
    .join(" ")
    .trim();
}

function timestamp(value: string | null | undefined): number {
  if (!value) return Number.NEGATIVE_INFINITY;
  const parsed = Date.parse(value);
  return Number.isNaN(parsed) ? Number.NEGATIVE_INFINITY : parsed;
}

function newestMatchingUnread(
  notifications: CmuxNotification[],
  pattern: RegExp,
): CmuxNotification | null {
  return notifications
    .filter((item) => !item.is_read && pattern.test(notificationText(item)))
    .sort((a, b) => timestamp(b.created_at) - timestamp(a.created_at))[0] || null;
}

export function normalizeManualStatus(value: unknown): AttentionStatus | null {
  switch (value) {
    case "needs_action":
    case "needs_review":
    case "executing":
    case "idle":
      return value;
    case "blocked":
      return "needs_action";
    case "review":
    case "verifying":
      return "needs_review";
    case "done":
      return "idle";
    default:
      return null;
  }
}

export function deriveTaskStatus(input: StatusInput): AttentionStatus {
  const manual = normalizeManualStatus(input.manualStatus);
  if (manual) return manual;

  if (input.agentEventKind === "question") return "needs_action";
  if (newestMatchingUnread(input.notifications, ACTION_PATTERN)) return "needs_action";
  if (input.backgroundShellProcess) return "executing";
  if (input.agentLifecycle === "running") return "executing";
  if (runningSurfaceSummary(input.activeSurfaceTitle)) return "executing";
  if (newestMatchingUnread(input.notifications, REVIEW_PATTERN)) return "needs_review";
  if (input.notifications.some((item) => !item.is_read)) return "needs_review";
  if (input.agentEventKind === "stop") {
    return timestamp(input.agentEventAt) > timestamp(input.lastViewedAt)
      ? "needs_review"
      : "idle";
  }
  if (waitingSurfaceSummary(input.activeSurfaceTitle) || input.agentLifecycle === "needsInput") {
    return input.lastViewedAt ? "idle" : "needs_review";
  }

  const newestTerminalAt = input.notifications.reduce((latest, item) => {
    const text = notificationText(item);
    if (!ACTION_PATTERN.test(text) && !REVIEW_PATTERN.test(text)) return latest;
    return Math.max(latest, timestamp(item.created_at));
  }, timestamp(input.lastViewedAt));

  if (timestamp(input.latestSubmittedAt) > newestTerminalAt) return "executing";
  return "idle";
}

export function statusReason(input: StatusInput): string | null {
  const action = newestMatchingUnread(input.notifications, ACTION_PATTERN);
  if (input.agentEventKind === "question") {
    const notification = input.notifications.find((item) => !item.is_read);
    return notification ? notificationText(notification) : "Claude 正在等待输入";
  }
  if (action) return notificationText(action);
  if (input.backgroundShellProcess) {
    return `后台 shell 仍在运行：${input.backgroundShellProcess}`;
  }
  if (input.agentLifecycle === "running" || runningSurfaceSummary(input.activeSurfaceTitle)) {
    return null;
  }
  const review = newestMatchingUnread(input.notifications, REVIEW_PATTERN);
  const notification = review || input.notifications.find((item) => !item.is_read) || null;
  if (notification) return notificationText(notification);
  if (input.agentEventKind === "stop" || waitingSurfaceSummary(input.activeSurfaceTitle)) {
    return "Claude 已完成，结果待查看";
  }
  return null;
}
