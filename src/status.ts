import type { CmuxNotification } from "./types";

export type AttentionStatus = "needs_action" | "needs_review" | "executing" | "idle";

export interface StatusInput {
  manualStatus: unknown;
  notifications: CmuxNotification[];
  latestSubmittedAt: string | null;
}

const ACTION_PATTERN = /\b(waiting|input required|needs? input|blocked|error|failed|failure)\b/i;
const REVIEW_PATTERN = /\b(completed|done|success|succeeded|finished)\b/i;

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

  if (newestMatchingUnread(input.notifications, ACTION_PATTERN)) return "needs_action";
  if (newestMatchingUnread(input.notifications, REVIEW_PATTERN)) return "needs_review";

  const newestTerminalAt = input.notifications.reduce((latest, item) => {
    const text = notificationText(item);
    if (!ACTION_PATTERN.test(text) && !REVIEW_PATTERN.test(text)) return latest;
    return Math.max(latest, timestamp(item.created_at));
  }, Number.NEGATIVE_INFINITY);

  if (timestamp(input.latestSubmittedAt) > newestTerminalAt) return "executing";
  return "idle";
}

export function statusReason(input: StatusInput): string | null {
  const action = newestMatchingUnread(input.notifications, ACTION_PATTERN);
  const review = newestMatchingUnread(input.notifications, REVIEW_PATTERN);
  const notification = action || review;
  return notification ? notificationText(notification) : null;
}
