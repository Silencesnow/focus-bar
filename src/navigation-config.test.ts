import { describe, expect, test } from "bun:test";
import {
  formFromTask,
  formsEqual,
  navigationErrorMessage,
  normalizeNavigationForm,
  validateNavigationForm,
  type NavigationForm,
} from "./navigation-config";
import type { TaskConfig } from "./types";

function emptyForm(overrides: Partial<NavigationForm> = {}): NavigationForm {
  return {
    name: "Task",
    tabIcon: "",
    chromeTargets: [],
    vscodeWorkspaceName: "",
    vscodeWorkspace: "",
    vscodeFile: "",
    vscodeLine: "",
    ...overrides,
  };
}

describe("normalizeNavigationForm", () => {
  test("normalizes a configured Tab icon", () => {
    expect(normalizeNavigationForm(emptyForm({ tabIcon: " 👨‍💻 " })).tab_icon)
      .toBe("👨‍💻");
  });

  test("normalizes empty target groups away", () => {
    expect(normalizeNavigationForm(emptyForm())).toEqual({
      name: "Task",
      tab_icon: null,
      chrome: null,
      vscode: null,
    });
  });

  test("normalizes multiple labeled Chrome targets", () => {
    expect(normalizeNavigationForm(emptyForm({
      chromeTargets: [
        { label: " Web MR ", url: " https://git.example.com/web/merge_requests/12 " },
        { label: "API MR", url: "https://git.example.com/api/merge_requests/34" },
      ],
    }))).toEqual({
      name: "Task",
      tab_icon: null,
      chrome: [
        { label: "Web MR", url: "https://git.example.com/web/merge_requests/12" },
        { label: "API MR", url: "https://git.example.com/api/merge_requests/34" },
      ],
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
      tab_icon: null,
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
    expect(validateNavigationForm(emptyForm({
      chromeTargets: [{ label: "Preview", url: "https://example.com/path?q=1#result" }],
    })))
      .toEqual([]);
  });

  test("accepts an absolute local file Chrome URL", () => {
    expect(validateNavigationForm(emptyForm({
      chromeTargets: [{
        label: "Html",
        url: "file:///Users/shamingming/Documents/work/myAgent/docs/mini-agent-stage-1.html",
      }],
    }))).toEqual([]);
  });

  test("rejects an invalid URL in any Chrome target", () => {
    expect(validateNavigationForm(emptyForm({
      chromeTargets: [
        { label: "Web MR", url: "https://example.com/valid" },
        { label: "API MR", url: "javascript:alert(1)" },
      ],
    })))
      .toContain("“API MR”必须是有效的 http、https 或本地 file:// URL");
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
    tab_icon: "FE",
    chrome: [
      { label: "Web MR", url: "https://example.com/web" },
      { label: "API MR", url: "https://example.com/api" },
    ],
    vscode: {
      workspace: "/tmp/app",
      workspace_name: "app",
      file: "src/main.ts",
      line: 7,
    },
  };
  const form = formFromTask(task);
  expect(form.tabIcon).toBe("FE");
  expect(form.chromeTargets).toEqual([
    { label: "Web MR", url: "https://example.com/web" },
    { label: "API MR", url: "https://example.com/api" },
  ]);
  expect(form.vscodeWorkspaceName).toBe("app");
  expect(form.vscodeLine).toBe("7");
});

test("prefills legacy single Chrome target", () => {
  const task = {
    id: "legacy",
    name: "Legacy",
    chrome: { url: "https://example.com/legacy" },
  } as TaskConfig;
  expect(formFromTask(task).chromeTargets).toEqual([
    { label: "example.com", url: "https://example.com/legacy" },
  ]);
});

test("detects dirty forms after trimming", () => {
  expect(formsEqual(emptyForm(), emptyForm({ name: " Task " }))).toBe(true);
  expect(formsEqual(emptyForm(), emptyForm({ tabIcon: "FE" }))).toBe(false);
  expect(formsEqual(emptyForm(), emptyForm({
    chromeTargets: [{ label: "Docs", url: "https://example.com" }],
  }))).toBe(false);
});

test("timeout guidance mentions a possible permission prompt", () => {
  expect(navigationErrorMessage({ code: "TARGET_TIMEOUT", message: "timed out" }))
    .toContain("权限弹窗");
});
