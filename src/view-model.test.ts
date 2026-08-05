import { describe, expect, test } from "bun:test";
import * as viewModel from "./view-model";
import { mergeWorkspaceTasks, sourceMessage } from "./view-model";
import type { CmuxNotification, CmuxWorkspace, TaskConfig } from "./types";

const workspace = (id: string, title: string): CmuxWorkspace => ({
  id,
  ref: `workspace:${id}`,
  title,
  current_directory: `/tmp/${title}`,
  listening_ports: [],
  latest_conversation_message: null,
  latest_submitted_at: null,
  selected: false,
  index: 1,
  window_id: "window:1",
});

const config = (id: string, name: string): TaskConfig => ({
  id: id.slice(0, 8),
  name,
  cmux_workspace_id: id,
  manual_status: null,
  note: "",
});

describe("mergeWorkspaceTasks", () => {
  test("renders the latest cmux title and hides historical configs", () => {
    const current = workspace("current-id", "Current");
    const tasks = mergeWorkspaceTasks(
      [current],
      [],
      [config("current-id", "Renamed"), config("old-id", "Old")],
    );
    expect(tasks).toHaveLength(1);
    expect(tasks[0].title).toBe("Current");
    expect(tasks[0].cmux?.id).toBe("current-id");
  });

  test("derives attention state from the workspace notifications", () => {
    const notification: CmuxNotification = {
      id: "n1", workspace_id: "current-id", title: "Agent",
      subtitle: "Waiting for input", body: "Please answer", is_read: false,
      created_at: "2026-07-10T10:00:00Z", tab_title: null,
    };
    const tasks = mergeWorkspaceTasks(
      [workspace("current-id", "Current")],
      [notification],
      [config("current-id", "Current")],
    );
    expect(tasks[0].effectiveStatus).toBe("needs_action");
    expect(tasks[0].statusReason).toContain("Please answer");
  });

  test("keeps an explicitly saved display-name override", () => {
    const current = workspace("current-id", "Current");
    const pinned = config("current-id", "Pinned");
    pinned.name_overridden = true;

    const tasks = mergeWorkspaceTasks([current], [], [pinned]);

    expect(tasks[0].title).toBe("Pinned");
  });

  test("builds activity from the latest message and newest activity timestamp", () => {
    const current = workspace("current-id", "Current");
    current.latest_conversation_message = "  正在修改 navigation.rs  ";
    current.latest_submitted_at = "2026-07-14T10:00:00Z";
    const notification: CmuxNotification = {
      id: "n1", workspace_id: "current-id", title: "Agent",
      subtitle: "Completed", body: "Done", is_read: true,
      created_at: "2026-07-14T10:02:00Z", tab_title: null,
    };

    const task = mergeWorkspaceTasks(
      [current],
      [notification],
      [config("current-id", "Current")],
    )[0] as ReturnType<typeof mergeWorkspaceTasks>[number] & {
      activitySummary?: string | null;
      activityAt?: string | null;
    };

    expect(task.activitySummary).toBe("正在修改 navigation.rs");
    expect(task.activityAt).toBe("2026-07-14T10:02:00Z");
  });

  test("falls back to notification text and ignores invalid activity times", () => {
    const current = workspace("current-id", "Current");
    current.latest_submitted_at = "not-a-date";
    const notification: CmuxNotification = {
      id: "n1", workspace_id: "current-id", title: "Agent",
      subtitle: "Waiting for input", body: "Please answer", is_read: false,
      created_at: "also-not-a-date", tab_title: null,
    };

    const task = mergeWorkspaceTasks(
      [current],
      [notification],
      [config("current-id", "Current")],
    )[0] as ReturnType<typeof mergeWorkspaceTasks>[number] & {
      activitySummary?: string | null;
      activityAt?: string | null;
    };

    expect(task.activitySummary).toBe("Waiting for input");
    expect(task.activityAt).toBeNull();
  });

  test("uses the running surface title when workspace submission metadata is missing", () => {
    const current = workspace("current-id", "Current");
    current.active_surface_title = "⠐ 支持yarn serve命令动态配置端口";
    const notification: CmuxNotification = {
      id: "n1", workspace_id: "current-id", title: "Claude Code",
      subtitle: "", body: "Claude is waiting for your input", is_read: true,
      created_at: "2026-07-14T06:39:21Z", tab_title: "ling-design-B",
    };

    const task = mergeWorkspaceTasks(
      [current],
      [notification],
      [config("current-id", "Current")],
    )[0];

    expect(task.effectiveStatus).toBe("executing");
    expect(task.activitySummary).toBe("支持yarn serve命令动态配置端口");
    expect(task.activityAt).toBeNull();
  });
});

test("formats relative activity time without throwing on invalid input", () => {
  const formatRelativeTime = (viewModel as unknown as {
    formatRelativeTime?: (value: string | null, now: number) => string | null;
  }).formatRelativeTime;

  expect(formatRelativeTime?.(
    "2026-07-14T10:00:00Z",
    Date.parse("2026-07-14T10:02:30Z"),
  )).toBe("2分钟前");
  expect(formatRelativeTime?.("invalid", Date.parse("2026-07-14T10:02:30Z"))).toBeNull();
});

test("access denied guidance names allowAll", () => {
  expect(sourceMessage({
    status: "error",
    code: "ACCESS_DENIED",
    message: "denied",
    detail: "broken pipe",
  })).toContain("allowAll");
});
