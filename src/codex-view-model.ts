import { normalizeManualStatus } from "./status";
import type { CodexThread, MergedTask, TaskConfig, TaskStatus } from "./types";

export const CODEX_IDLE_RETENTION_MS = 24 * 60 * 60 * 1_000;

function fallbackConfig(thread: CodexThread): TaskConfig {
  return {
    id: `codex-${thread.id.slice(0, 8)}`,
    name: thread.title || thread.cwd.split("/").pop() || "Codex task",
    name_overridden: false,
    codex_thread_id: thread.id,
    codex_directory: thread.cwd,
    manual_status: null,
    note: "",
  };
}

function isExplicitlyConfigured(config: TaskConfig | undefined): boolean {
  return !!config && !!(
    config.name_overridden
    || config.manual_status
    || config.note?.trim()
    || config.vscode
    || config.chrome
  );
}

function statusForThread(
  thread: CodexThread,
  config: TaskConfig,
  now: number,
): TaskStatus {
  const manual = normalizeManualStatus(config.manual_status);
  if (manual) return manual;
  if (thread.lifecycle === "needs_input" || thread.lifecycle === "failed") return "needs_action";
  if (thread.lifecycle === "executing") return "executing";
  if (thread.lifecycle === "completed") {
    const completedAt = thread.activity_at || thread.updated_at;
    const viewedAt = config.last_viewed_at ? Date.parse(config.last_viewed_at) : Number.NaN;
    if (Number.isFinite(viewedAt)) return completedAt > viewedAt ? "needs_review" : "idle";
    return now - completedAt <= CODEX_IDLE_RETENTION_MS ? "needs_review" : "idle";
  }
  return "idle";
}

function shouldShow(
  thread: CodexThread,
  status: TaskStatus,
  configured: boolean,
  now: number,
): boolean {
  if (configured || status !== "idle") return true;
  return now - thread.updated_at <= CODEX_IDLE_RETENTION_MS;
}

function activitySummary(thread: CodexThread): string | null {
  if (thread.latest_message?.trim()) return thread.latest_message.trim();
  switch (thread.lifecycle) {
    case "executing": return "Codex 正在执行";
    case "needs_input": return "Codex 正在等待输入";
    case "failed": return "Codex 执行失败";
    case "completed": return "Codex 已完成";
    case "idle": return null;
  }
}

function statusReason(thread: CodexThread, status: TaskStatus): string | null {
  if (thread.lifecycle === "needs_input") return "Codex 正在等待输入";
  if (thread.lifecycle === "failed") return "Codex 执行失败";
  if (status === "needs_review") return "Codex 已完成，结果待查看";
  return null;
}

export function mergeCodexTasks(
  threads: CodexThread[],
  configs: TaskConfig[],
  now = Date.now(),
): MergedTask[] {
  return threads.flatMap((thread) => {
    const saved = configs.find((config) => config.codex_thread_id === thread.id);
    const config = saved || fallbackConfig(thread);
    const status = statusForThread(thread, config, now);
    if (!shouldShow(thread, status, isExplicitlyConfigured(saved), now)) return [];
    const activityAt = thread.activity_at || thread.updated_at;
    return [{
      config,
      source: "codex" as const,
      codex: thread,
      notifications: [],
      hasUnread: status === "needs_review",
      latestNotifSubtitle: null,
      effectiveStatus: status,
      ports: [],
      directory: thread.cwd,
      title: config.name_overridden && config.name.trim()
        ? config.name.trim()
        : thread.title.trim() || config.name.trim() || thread.cwd.split("/").pop() || "Codex task",
      latestMessage: thread.latest_message,
      statusReason: statusReason(thread, status),
      activitySummary: activitySummary(thread),
      activityAt: activityAt > 0 ? new Date(activityAt).toISOString() : null,
    }];
  });
}
