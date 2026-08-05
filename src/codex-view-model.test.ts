import { describe, expect, test } from "bun:test";
import { mergeCodexTasks } from "./codex-view-model";
import type { CodexThread, TaskConfig } from "./types";

const NOW = Date.parse("2026-07-16T12:00:00Z");

function thread(overrides: Partial<CodexThread> = {}): CodexThread {
  return {
    id: "019f-thread",
    title: "实现 Codex 数据源",
    cwd: "/tmp/focus-bar",
    lifecycle: "completed",
    updated_at: Date.parse("2026-07-16T11:50:00Z"),
    activity_at: Date.parse("2026-07-16T11:50:00Z"),
    latest_message: "实现完成",
    ...overrides,
  };
}

function config(overrides: Partial<TaskConfig> = {}): TaskConfig {
  return {
    id: "codex-019f",
    name: "实现 Codex 数据源",
    codex_thread_id: "019f-thread",
    manual_status: null,
    ...overrides,
  };
}

describe("mergeCodexTasks", () => {
  test("keeps a silent turn executing until Codex records completion", () => {
    const tasks = mergeCodexTasks([
      thread({
        lifecycle: "executing",
        activity_at: Date.parse("2026-07-15T08:00:00Z"),
        updated_at: Date.parse("2026-07-15T08:00:00Z"),
      }),
    ], [config()], NOW);

    expect(tasks).toHaveLength(1);
    expect(tasks[0].effectiveStatus).toBe("executing");
    expect(tasks[0].source).toBe("codex");
  });

  test("shows a completed turn as review until it has been viewed", () => {
    const unread = mergeCodexTasks([
      thread({ activity_at: Date.parse("2026-07-16T11:50:00Z") }),
    ], [config({ last_viewed_at: "2026-07-16T11:00:00Z" })], NOW)[0];
    const viewed = mergeCodexTasks([
      thread({ activity_at: Date.parse("2026-07-16T11:50:00Z") }),
    ], [config({ last_viewed_at: "2026-07-16T11:55:00Z" })], NOW)[0];

    expect(unread.effectiveStatus).toBe("needs_review");
    expect(viewed.effectiveStatus).toBe("idle");
  });

  test("maps explicit input waits and failures to action", () => {
    const waiting = mergeCodexTasks([
      thread({ lifecycle: "needs_input" }),
    ], [config()], NOW)[0];
    const failed = mergeCodexTasks([
      thread({ lifecycle: "failed" }),
    ], [config()], NOW)[0];

    expect(waiting.effectiveStatus).toBe("needs_action");
    expect(failed.effectiveStatus).toBe("needs_action");
  });

  test("hides stale idle threads but never hides active or configured threads", () => {
    const old = Date.parse("2026-07-14T10:00:00Z");
    const tasks = mergeCodexTasks([
      thread({ id: "old-idle", lifecycle: "completed", updated_at: old, activity_at: old }),
      thread({ id: "old-active", lifecycle: "executing", updated_at: old, activity_at: old }),
      thread({ id: "old-configured", lifecycle: "completed", updated_at: old, activity_at: old }),
    ], [
      config({ id: "configured", codex_thread_id: "old-configured", chrome: { url: "https://example.com" } }),
    ], NOW);

    expect(tasks.map((task) => task.codex?.id)).toEqual(["old-active", "old-configured"]);
  });
});
