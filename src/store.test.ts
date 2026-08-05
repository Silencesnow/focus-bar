import { expect, test } from "bun:test";
import * as store from "./store";
import type { CmuxWorkspace, CodexThread, FocusData } from "./types";

function workspace(id: string, directory: string): CmuxWorkspace {
  return {
    id,
    ref: `workspace:${id}`,
    title: "Current task",
    current_directory: directory,
    listening_ports: [],
    latest_conversation_message: null,
    latest_submitted_at: null,
    selected: false,
    index: 0,
    window_id: "window:1",
  };
}

test("rebinds a uniquely matching saved config when cmux assigns a new workspace id", () => {
  const reconcile = (store as unknown as {
    reconcileWorkspaceConfigs?: (
      workspaces: CmuxWorkspace[],
      data: FocusData,
    ) => { data: FocusData; workspaceConfigs: FocusData["tasks"]; changed: boolean };
  }).reconcileWorkspaceConfigs;
  const data: FocusData = {
    tasks: [{
      id: "saved-task",
      name: "Saved task",
      cmux_workspace_id: "old-workspace",
      cmux_directory: "/tmp/project/",
      manual_status: "idle",
      last_viewed_at: "2026-07-21T07:07:09.701Z",
      chrome: [{ label: "MR", url: "https://example.com/mr" }],
    }],
  };

  const result = reconcile?.([workspace("new-workspace", "/tmp/project")], data);

  expect(result?.workspaceConfigs[0].id).toBe("saved-task");
  expect(result?.workspaceConfigs[0].cmux_workspace_id).toBe("new-workspace");
  expect(result?.workspaceConfigs[0].chrome).toEqual(data.tasks[0].chrome);
  expect(result?.workspaceConfigs[0].manual_status).toBe(null);
  expect(result?.workspaceConfigs[0].last_viewed_at).toBe(null);
  expect(result?.changed).toBe(true);
});

test("does not steal a config from another currently open workspace with the same directory", () => {
  const reconcile = (store as unknown as {
    reconcileWorkspaceConfigs?: (
      workspaces: CmuxWorkspace[],
      data: FocusData,
    ) => { workspaceConfigs: FocusData["tasks"] };
  }).reconcileWorkspaceConfigs;
  const data: FocusData = {
    tasks: [{
      id: "saved-task",
      name: "Saved task",
      cmux_workspace_id: "still-open",
      cmux_directory: "/tmp/project",
      vscode: { workspace: "/tmp/project" },
    }],
  };

  const result = reconcile?.([
    workspace("still-open", "/tmp/project"),
    workspace("new-workspace", "/tmp/project"),
  ], data);

  expect(result?.workspaceConfigs[0].id).toBe("saved-task");
  expect(result?.workspaceConfigs[1].id).not.toBe("saved-task");
});

test("clears a legacy temporary status that is not scoped to the current workspace", () => {
  const data: FocusData = { tasks: [{
    id: "task-a",
    name: "Task A",
    cmux_workspace_id: "workspace-a",
    manual_status: "idle",
  }] };

  const result = store.reconcileWorkspaceConfigs([workspace("workspace-a", "/tmp/a")], data);

  expect(result.workspaceConfigs[0].manual_status).toBe(null);
});

test("keeps a temporary status explicitly scoped to the current workspace", () => {
  const data: FocusData = { tasks: [{
    id: "task-a",
    name: "Task A",
    cmux_workspace_id: "workspace-a",
    manual_status: "idle",
    manual_status_context_id: "workspace-a",
  }] };

  const result = store.reconcileWorkspaceConfigs([workspace("workspace-a", "/tmp/a")], data);

  expect(result.workspaceConfigs[0].manual_status).toBe("idle");
});

test("records when a task was successfully viewed", () => {
  const recordTaskViewed = (store as unknown as {
    recordTaskViewed?: (data: FocusData, taskId: string, viewedAt: string) => boolean;
  }).recordTaskViewed;
  const data: FocusData = { tasks: [{ id: "task-1", name: "Task" }] };

  const changed = recordTaskViewed?.(data, "task-1", "2026-07-14T10:00:00Z");

  expect(changed).toBe(true);
  expect(data.tasks[0].last_viewed_at).toBe("2026-07-14T10:00:00Z");
});

test("records when an idle task is manually collapsed", () => {
  const recordTaskCollapsed = (store as unknown as {
    recordTaskCollapsed?: (data: FocusData, taskId: string, collapsedAt: string) => boolean;
  }).recordTaskCollapsed;
  const data: FocusData = { tasks: [{ id: "task-1", name: "Task" }] };

  const changed = recordTaskCollapsed?.(data, "task-1", "2026-07-27T10:00:00Z");

  expect(changed).toBe(true);
  expect(
    (data.tasks[0] as FocusData["tasks"][number] & { manually_collapsed_at?: string })
      .manually_collapsed_at,
  ).toBe("2026-07-27T10:00:00Z");
});

test("creates Codex configs by thread id without losing saved navigation", () => {
  const reconcile = (store as unknown as {
    reconcileCodexConfigs?: (
      threads: CodexThread[],
      data: FocusData,
    ) => { data: FocusData; threadConfigs: FocusData["tasks"]; changed: boolean };
  }).reconcileCodexConfigs;
  const threads: CodexThread[] = [
    {
      id: "thread-saved", title: "Saved", cwd: "/tmp/project",
      lifecycle: "completed", updated_at: 1, activity_at: 1, latest_message: null,
    },
    {
      id: "thread-new", title: "New", cwd: "/tmp/project",
      lifecycle: "executing", updated_at: 2, activity_at: 2, latest_message: null,
    },
  ];
  const data: FocusData = { tasks: [{
    id: "saved-codex",
    name: "Saved override",
    name_overridden: true,
    codex_thread_id: "thread-saved",
    chrome: { label: "MR", url: "https://example.com/mr" },
  }] };

  const result = reconcile?.(threads, data);

  expect(result?.threadConfigs[0].id).toBe("saved-codex");
  expect(result?.threadConfigs[0].chrome).toEqual(data.tasks[0].chrome);
  expect(result?.threadConfigs[1].codex_thread_id).toBe("thread-new");
  expect(result?.threadConfigs[1].id).not.toBe("saved-codex");
  expect(result?.changed).toBe(true);
});
