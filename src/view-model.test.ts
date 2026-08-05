import { describe, expect, test } from "bun:test";
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
});

test("access denied guidance names allowAll", () => {
  expect(sourceMessage({
    status: "error",
    code: "ACCESS_DENIED",
    message: "denied",
    detail: "broken pipe",
  })).toContain("allowAll");
});
