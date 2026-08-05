import { expect, test } from "bun:test";
import { buildSettingsTasks } from "./settings-tasks";
import type { CmuxWorkspace, CodexThread, TaskConfig } from "./types";

test("lists cmux and Codex tasks separately even when they share a directory", () => {
  const workspace: CmuxWorkspace = {
    id: "workspace-1", ref: "workspace:1", title: "cmux task",
    current_directory: "/tmp/project", listening_ports: [],
    latest_conversation_message: null, latest_submitted_at: null,
    selected: false, index: 0, window_id: "window:1",
  };
  const thread: CodexThread = {
    id: "thread-1", title: "Codex task", cwd: "/tmp/project",
    lifecycle: "executing", updated_at: 1, activity_at: 1, latest_message: null,
  };
  const configs: TaskConfig[] = [
    { id: "cmux-config", name: "cmux task", cmux_workspace_id: "workspace-1" },
    { id: "codex-config", name: "Codex task", codex_thread_id: "thread-1" },
  ];

  const tasks = buildSettingsTasks([workspace], [thread], configs);

  expect(tasks.map((task) => task.source)).toEqual(["cmux", "codex"]);
  expect(tasks.map((task) => task.config.id)).toEqual(["cmux-config", "codex-config"]);
  expect(tasks.map((task) => task.runtimeTitle)).toEqual(["cmux task", "Codex task"]);
});
