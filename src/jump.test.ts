import { expect, test } from "bun:test";
import { codexThreadUrl, resolveChromeUrl, smartJumpTargets } from "./jump";
import type { MergedTask } from "./types";

function taskWithChrome(): MergedTask {
  return {
    config: {
      id: "task",
      name: "Task",
      chrome: [
        { label: "Web MR", url: "https://git.example.com/web/12" },
        { label: "API MR", url: "https://git.example.com/api/34" },
      ],
    },
    notifications: [],
    hasUnread: false,
    latestNotifSubtitle: null,
    effectiveStatus: "needs_review",
    ports: [],
    directory: "/tmp/task",
    title: "Task",
    latestMessage: null,
    statusReason: null,
  };
}

test("resolves the clicked Chrome target instead of always using the first", () => {
  expect(resolveChromeUrl(taskWithChrome(), 1)).toBe("https://git.example.com/api/34");
});

test("falls back to the first listening port when no Chrome target exists", () => {
  const task = taskWithChrome();
  task.config.chrome = undefined;
  task.ports = [4173];
  expect(resolveChromeUrl(task)).toBe("http://localhost:4173");
});

test("builds the official Codex desktop deep link", () => {
  expect(codexThreadUrl("019f4a17-0fe9-7aa3-a822-8adf777fb979"))
    .toBe("codex://threads/019f4a17-0fe9-7aa3-a822-8adf777fb979");
});

test("a pending cmux task card returns to cmux before VS Code", () => {
  const task = taskWithChrome();
  task.cmux = {
    id: "workspace-c",
    ref: "workspace:2",
    title: "ling-design-C",
    current_directory: "/tmp/task",
    listening_ports: [],
    latest_conversation_message: null,
    latest_activity_at: null,
    latest_submitted_at: null,
    selected: false,
    index: 1,
    window_id: "window:1",
  };
  task.config.cmux_workspace_id = "workspace-c";
  task.config.vscode = { workspace: "/tmp/task" };
  task.effectiveStatus = "needs_review";

  expect(smartJumpTargets(task)).toEqual(["cmux", "vscode", "chrome"]);
});
