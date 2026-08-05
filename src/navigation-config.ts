import type {
  ChromeTarget,
  NavigationError,
  TaskConfig,
  VscodeTarget,
} from "./types";

export interface NavigationForm {
  name: string;
  chromeUrl: string;
  vscodeWorkspaceName: string;
  vscodeWorkspace: string;
  vscodeFile: string;
  vscodeLine: string;
}

export interface NormalizedNavigation {
  name: string;
  chrome: ChromeTarget | null;
  vscode: VscodeTarget | null;
}

function basename(path: string): string {
  return path.replace(/\/+$/, "").split("/").pop() || path;
}

function hasEscapingSegment(path: string): boolean {
  return path.split("/").some((segment) => segment === "..");
}

export function formFromTask(task: TaskConfig): NavigationForm {
  return {
    name: task.name || "",
    chromeUrl: task.chrome?.url || "",
    vscodeWorkspaceName: task.vscode?.workspace_name || "",
    vscodeWorkspace: task.vscode?.workspace || "",
    vscodeFile: task.vscode?.file || "",
    vscodeLine: task.vscode?.line ? String(task.vscode.line) : "",
  };
}

export function normalizeNavigationForm(form: NavigationForm): NormalizedNavigation {
  const name = form.name.trim();
  const chromeUrl = form.chromeUrl.trim();
  const workspace = form.vscodeWorkspace.trim();
  const workspaceName = form.vscodeWorkspaceName.trim();
  const file = form.vscodeFile.trim();
  const lineText = form.vscodeLine.trim();

  return {
    name,
    chrome: chromeUrl ? { url: chromeUrl } : null,
    vscode: workspace
      ? {
          workspace,
          workspace_name: workspaceName || basename(workspace),
          ...(file ? { file } : {}),
          ...(lineText ? { line: Number(lineText) } : {}),
        }
      : null,
  };
}

export function validateNavigationForm(form: NavigationForm): string[] {
  const errors: string[] = [];
  const chromeUrl = form.chromeUrl.trim();
  if (chromeUrl) {
    try {
      const url = new URL(chromeUrl);
      if ((url.protocol !== "http:" && url.protocol !== "https:") || !url.host) throw new Error();
    } catch {
      errors.push("Chrome 链接必须是有效的 http 或 https URL");
    }
  }

  const workspace = form.vscodeWorkspace.trim();
  const file = form.vscodeFile.trim();
  const line = form.vscodeLine.trim();
  if (workspace && !workspace.startsWith("/")) {
    errors.push("VS Code workspace 必须是绝对目录");
  }
  if (file && (file.startsWith("/") || hasEscapingSegment(file))) {
    errors.push("文件路径不能离开 workspace");
  }
  if (line && (!/^\d+$/.test(line) || Number(line) <= 0)) {
    errors.push("行号必须是正整数");
  }
  if ((file || line || form.vscodeWorkspaceName.trim()) && !workspace) {
    errors.push("配置 VS Code 文件、行号或名称时必须填写 workspace 目录");
  }
  return errors;
}

export function formsEqual(a: NavigationForm, b: NavigationForm): boolean {
  return JSON.stringify(normalizeNavigationForm(a)) === JSON.stringify(normalizeNavigationForm(b));
}

export function navigationErrorMessage(error: NavigationError): string {
  switch (error.code) {
    case "INVALID_TARGET": return error.message || "跳转目标配置无效";
    case "CHROME_NOT_INSTALLED": return "找不到 Google Chrome。";
    case "VSCODE_NOT_INSTALLED": return "找不到官方 VS Code 或 code 命令。";
    case "AUTOMATION_PERMISSION_REQUIRED": return "请在系统设置 → 隐私与安全性 → 自动化中允许 Focus Bar 控制 Google Chrome。";
    case "ACCESSIBILITY_PERMISSION_REQUIRED": return "请在系统设置 → 隐私与安全性 → 辅助功能中允许 Focus Bar 控制 VS Code 窗口。";
    case "TARGET_TIMEOUT": return "目标应用响应超时。";
    case "TARGET_COMMAND_FAILED": return error.message || "跳转目标失败。";
  }
}
