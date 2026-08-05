import type {
  ChromeTarget,
  NavigationError,
  TaskConfig,
  VscodeTarget,
} from "./types";
import { normalizeTabIcon } from "./tab-icon";

export interface NavigationForm {
  name: string;
  tabIcon: string;
  chromeTargets: ChromeTarget[];
  vscodeWorkspaceName: string;
  vscodeWorkspace: string;
  vscodeFile: string;
  vscodeLine: string;
}

export interface NormalizedNavigation {
  name: string;
  tab_icon: string | null;
  chrome: ChromeTarget[] | null;
  vscode: VscodeTarget | null;
}

function basename(path: string): string {
  return path.replace(/\/+$/, "").split("/").pop() || path;
}

function hasEscapingSegment(path: string): boolean {
  return path.split("/").some((segment) => segment === "..");
}

function chromeLabel(url: string, index: number): string {
  try {
    return new URL(url).hostname || `链接 ${index + 1}`;
  } catch {
    return `链接 ${index + 1}`;
  }
}

export function chromeTargetsFromTask(task: TaskConfig): ChromeTarget[] {
  if (!task.chrome) return [];
  const targets = Array.isArray(task.chrome) ? task.chrome : [task.chrome];
  return targets.map((target, index) => ({
    label: target.label?.trim() || chromeLabel(target.url, index),
    url: target.url,
  }));
}

export function formFromTask(task: TaskConfig): NavigationForm {
  return {
    name: task.name || "",
    tabIcon: task.tab_icon || "",
    chromeTargets: chromeTargetsFromTask(task),
    vscodeWorkspaceName: task.vscode?.workspace_name || "",
    vscodeWorkspace: task.vscode?.workspace || "",
    vscodeFile: task.vscode?.file || "",
    vscodeLine: task.vscode?.line ? String(task.vscode.line) : "",
  };
}

export function normalizeNavigationForm(form: NavigationForm): NormalizedNavigation {
  const name = form.name.trim();
  const tabIcon = normalizeTabIcon(form.tabIcon);
  const chrome = form.chromeTargets
    .map((target, index) => {
      const url = target.url.trim();
      return {
        label: target.label?.trim() || chromeLabel(url, index),
        url,
      };
    })
    .filter((target) => target.url);
  const workspace = form.vscodeWorkspace.trim();
  const workspaceName = form.vscodeWorkspaceName.trim();
  const file = form.vscodeFile.trim();
  const lineText = form.vscodeLine.trim();

  return {
    name,
    tab_icon: tabIcon || null,
    chrome: chrome.length ? chrome : null,
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
  form.chromeTargets.forEach((target, index) => {
    const urlValue = target.url.trim();
    const label = target.label?.trim() || `Chrome 链接 ${index + 1}`;
    if (!urlValue) {
      if (target.label?.trim()) errors.push(`“${label}”缺少 URL`);
      return;
    }
    try {
      const url = new URL(urlValue);
      const isWebUrl = (url.protocol === "http:" || url.protocol === "https:") && Boolean(url.host);
      const isLocalFileUrl = url.protocol === "file:" && !url.host && url.pathname.startsWith("/");
      if (!isWebUrl && !isLocalFileUrl) throw new Error();
    } catch {
      errors.push(`“${label}”必须是有效的 http、https 或本地 file:// URL`);
    }
  });

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
    case "TARGET_TIMEOUT": return "目标应用响应超时；请检查是否有等待处理的 macOS 权限弹窗。";
    case "TARGET_COMMAND_FAILED": return error.message || "跳转目标失败。";
  }
}
