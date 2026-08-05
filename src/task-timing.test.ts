import { describe, expect, test } from "bun:test";
import { taskStatusSamples } from "./task-timing";
import type { MergedTask, TaskStatus } from "./types";

function mergedTask(overrides: Partial<MergedTask> = {}): MergedTask {
  return {
    config: { id: "task-1", name: "任务一", manual_status: null },
    source: "cmux",
    notifications: [],
    hasUnread: false,
    latestNotifSubtitle: null,
    effectiveStatus: "executing",
    ports: [],
    directory: "/tmp/one",
    title: "任务一",
    latestMessage: null,
    statusReason: null,
    activitySummary: null,
    activityAt: null,
    ...overrides,
  };
}

describe("taskStatusSamples", () => {
  test("maps cmux and Codex tasks identically into snapshot samples", () => {
    const statuses: TaskStatus[] = ["needs_action", "needs_review", "executing", "idle"];
    const cmux = statuses.map((status, index) =>
      mergedTask({
        config: { id: `cmux-${index}`, name: `cmux ${index}`, manual_status: null },
        source: "cmux",
        effectiveStatus: status,
        title: `cmux ${index}`,
      }),
    );
    const codex = statuses.map((status, index) =>
      mergedTask({
        config: { id: `codex-${index}`, name: `codex ${index}`, manual_status: null },
        source: "codex",
        effectiveStatus: status,
        title: `codex ${index}`,
      }),
    );

    const samples = taskStatusSamples(cmux, codex);

    expect(samples).toEqual([
      { task_id: "cmux-0", task_title: "cmux 0", source: "cmux", status: "needs_action" },
      { task_id: "cmux-1", task_title: "cmux 1", source: "cmux", status: "needs_review" },
      { task_id: "cmux-2", task_title: "cmux 2", source: "cmux", status: "executing" },
      { task_id: "cmux-3", task_title: "cmux 3", source: "cmux", status: "idle" },
      { task_id: "codex-0", task_title: "codex 0", source: "codex", status: "needs_action" },
      { task_id: "codex-1", task_title: "codex 1", source: "codex", status: "needs_review" },
      { task_id: "codex-2", task_title: "codex 2", source: "codex", status: "executing" },
      { task_id: "codex-3", task_title: "codex 3", source: "codex", status: "idle" },
    ]);
  });

  test("excludes tasks without a stable id and tags source by list", () => {
    const samples = taskStatusSamples(
      [mergedTask({ config: { id: "", name: "无 id", manual_status: null } })],
      [
        mergedTask({
          config: { id: "task-2", name: "无来源", manual_status: null },
          source: undefined,
          title: "Codex 任务",
          effectiveStatus: "idle",
        }),
      ],
    );

    expect(samples).toEqual([
      { task_id: "task-2", task_title: "Codex 任务", source: "codex", status: "idle" },
    ]);
  });
});
