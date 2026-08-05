import { describe, expect, test } from "bun:test";
import {
  formFromTask,
  formsEqual,
  normalizeNavigationForm,
  validateNavigationForm,
  type NavigationForm,
} from "./navigation-config";
import type { TaskConfig } from "./types";

function emptyForm(overrides: Partial<NavigationForm> = {}): NavigationForm {
  return {
    name: "Task",
    chromeUrl: "",
    vscodeWorkspaceName: "",
    vscodeWorkspace: "",
    vscodeFile: "",
    vscodeLine: "",
    ...overrides,
  };
}

describe("normalizeNavigationForm", () => {
  test("normalizes empty target groups away", () => {
    expect(normalizeNavigationForm(emptyForm())).toEqual({
      name: "Task",
      chrome: null,
      vscode: null,
    });
  });

  test("derives workspace name and parses line", () => {
    expect(normalizeNavigationForm(emptyForm({
      vscodeWorkspace: "/tmp/my-app",
      vscodeFile: "src/main.ts",
      vscodeLine: "42",
    }))).toEqual({
      name: "Task",
      chrome: null,
      vscode: {
        workspace: "/tmp/my-app",
        workspace_name: "my-app",
        file: "src/main.ts",
        line: 42,
      },
    });
  });
});

describe("validateNavigationForm", () => {
  test("accepts an exact https Chrome URL", () => {
    expect(validateNavigationForm(emptyForm({ chromeUrl: "https://example.com/path?q=1#result" })))
      .toEqual([]);
  });

  test("rejects non-http Chrome URL", () => {
    expect(validateNavigationForm(emptyForm({ chromeUrl: "javascript:alert(1)" })))
      .toContain("Chrome 链接必须是有效的 http 或 https URL");
  });

  test("rejects a relative VS Code workspace", () => {
    expect(validateNavigationForm(emptyForm({ vscodeWorkspace: "packages/app" })))
      .toContain("VS Code workspace 必须是绝对目录");
  });

  test("rejects a file that escapes its workspace", () => {
    expect(validateNavigationForm(emptyForm({
      vscodeWorkspace: "/tmp/app",
      vscodeFile: "../secret",
    }))).toContain("文件路径不能离开 workspace");
  });

  test("rejects a non-positive line", () => {
    expect(validateNavigationForm(emptyForm({
      vscodeWorkspace: "/tmp/app",
      vscodeLine: "0",
    }))).toContain("行号必须是正整数");
  });
});

test("prefills existing navigation fields", () => {
  const task: TaskConfig = {
    id: "task",
    name: "Configured",
    chrome: { url: "https://example.com" },
    vscode: {
      workspace: "/tmp/app",
      workspace_name: "app",
      file: "src/main.ts",
      line: 7,
    },
  };
  const form = formFromTask(task);
  expect(form.chromeUrl).toBe("https://example.com");
  expect(form.vscodeWorkspaceName).toBe("app");
  expect(form.vscodeLine).toBe("7");
});

test("detects dirty forms after trimming", () => {
  expect(formsEqual(emptyForm(), emptyForm({ name: " Task " }))).toBe(true);
  expect(formsEqual(emptyForm(), emptyForm({ chromeUrl: "https://example.com" }))).toBe(false);
});
