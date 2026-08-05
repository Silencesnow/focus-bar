import { expect, test } from "bun:test";
import { resolveChromeUrl } from "./jump";
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
