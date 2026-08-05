import { describe, expect, test } from "bun:test";
import {
  findTaskByConfigId,
  groupTasksByToday,
  taskListModel,
} from "./task-groups";
import type { MergedTask, TaskConfig, TaskStatus } from "./types";

const NOW = new Date(2026, 6, 27, 12, 0, 0).getTime();

function task(
  id: string,
  effectiveStatus: TaskStatus,
  activityAt: string | null,
  configOverrides: Partial<TaskConfig> = {},
): MergedTask {
  return {
    config: { id, name: id, ...configOverrides },
    notifications: [],
    hasUnread: false,
    latestNotifSubtitle: null,
    effectiveStatus,
    ports: [],
    directory: `/tmp/${id}`,
    title: id,
    latestMessage: null,
    statusReason: null,
    activitySummary: null,
    activityAt,
  };
}

describe("groupTasksByToday", () => {
  test("keeps an idle task with local-today activity in the current list", () => {
    const today = new Date(2026, 6, 27, 8, 30, 0).toISOString();

    const groups = groupTasksByToday([task("today", "idle", today)], NOW);

    expect(groups.current.map((item) => item.config.id)).toEqual(["today"]);
    expect(groups.inactive).toEqual([]);
  });

  test("archives idle tasks whose activity is old, missing, or invalid", () => {
    const yesterday = new Date(2026, 6, 26, 23, 59, 59).toISOString();
    const tasks = [
      task("old", "idle", yesterday),
      task("missing", "idle", null),
      task("invalid", "idle", "not-a-date"),
    ];

    const groups = groupTasksByToday(tasks, NOW);

    expect(groups.current).toEqual([]);
    expect(groups.inactive.map((item) => item.config.id)).toEqual([
      "old",
      "missing",
      "invalid",
    ]);
  });

  test("never archives pending or executing tasks", () => {
    const old = new Date(2026, 6, 20, 10, 0, 0).toISOString();
    const tasks = [
      task("action", "needs_action", old),
      task("review", "needs_review", null),
      task("running", "executing", "not-a-date"),
    ];

    const groups = groupTasksByToday(tasks, NOW);

    expect(groups.current.map((item) => item.config.id)).toEqual([
      "action",
      "review",
      "running",
    ]);
    expect(groups.inactive).toEqual([]);
  });

  test("archives an idle task manually collapsed after its latest activity", () => {
    const activityAt = new Date(2026, 6, 27, 9, 0, 0).toISOString();
    const collapsedAt = new Date(2026, 6, 27, 10, 0, 0).toISOString();
    const manuallyCollapsed = {
      manually_collapsed_at: collapsedAt,
    } as Partial<TaskConfig>;

    const groups = groupTasksByToday([
      task("finished", "idle", activityAt, manuallyCollapsed),
    ], NOW);

    expect(groups.current).toEqual([]);
    expect(groups.inactive.map((item) => item.config.id)).toEqual(["finished"]);
  });

  test("restores a manually collapsed idle task after newer activity", () => {
    const collapsedAt = new Date(2026, 6, 27, 9, 0, 0).toISOString();
    const activityAt = new Date(2026, 6, 27, 10, 0, 0).toISOString();
    const manuallyCollapsed = {
      manually_collapsed_at: collapsedAt,
    } as Partial<TaskConfig>;

    const groups = groupTasksByToday([
      task("reactivated", "idle", activityAt, manuallyCollapsed),
    ], NOW);

    expect(groups.current.map((item) => item.config.id)).toEqual(["reactivated"]);
    expect(groups.inactive).toEqual([]);
  });

  test("keeps pending and executing tasks visible despite a collapse marker", () => {
    const collapsedAt = new Date(2026, 6, 27, 11, 0, 0).toISOString();
    const activityAt = new Date(2026, 6, 27, 10, 0, 0).toISOString();
    const manuallyCollapsed = {
      manually_collapsed_at: collapsedAt,
    } as Partial<TaskConfig>;
    const tasks = [
      task("pending", "needs_review", activityAt, manuallyCollapsed),
      task("running", "executing", activityAt, manuallyCollapsed),
    ];

    const groups = groupTasksByToday(tasks, NOW);

    expect(groups.current.map((item) => item.config.id)).toEqual([
      "pending",
      "running",
    ]);
    expect(groups.inactive).toEqual([]);
  });
});

test("taskListModel hides older tasks until the section is expanded", () => {
  const today = new Date(2026, 6, 27, 9, 0, 0).toISOString();
  const yesterday = new Date(2026, 6, 26, 9, 0, 0).toISOString();
  const tasks = [
    task("current", "idle", today),
    task("older", "idle", yesterday),
  ];

  const collapsed = taskListModel(tasks, false, NOW);
  const expanded = taskListModel(tasks, true, NOW);

  expect(collapsed.visible.map((item) => item.config.id)).toEqual(["current"]);
  expect(collapsed.inactiveToggle).toEqual({
    count: 1,
    label: "较早任务（1）",
    expanded: false,
  });
  expect(expanded.visible.map((item) => item.config.id)).toEqual(["current", "older"]);
  expect(expanded.inactiveToggle?.expanded).toBe(true);
});

test("findTaskByConfigId resolves the same task after rendering is regrouped", () => {
  const tasks = [
    task("workspace-a", "idle", null),
    task("workspace-b", "needs_review", null),
  ];

  expect(findTaskByConfigId(tasks, "workspace-a")?.title).toBe("workspace-a");
  expect(findTaskByConfigId(tasks, "missing")).toBeNull();
});
