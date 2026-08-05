import { invoke } from "@tauri-apps/api/core";
import { chromeTargetsFromTask } from "./navigation-config";
import type { ChromeTarget, MergedTask } from "./types";
import { focusWorkspace } from "./cmux";

export async function jumpToCmux(task: MergedTask): Promise<void> {
  const wsRef = task.cmux?.ref;
  const wsId = task.cmux?.id || task.config.cmux_workspace_id;
  const winId = task.cmux?.window_id || "";
  if (!wsRef && !wsId) throw new Error("No cmux workspace for this task");
  await focusWorkspace(wsRef || wsId || "", wsId || "", winId);
}

export async function jumpToVscode(task: MergedTask): Promise<void> {
  const vscode = task.config.vscode;
  if (!vscode) throw new Error("No VS Code target configured");
  await invoke("focus_vscode_target", {
    workspace: vscode.workspace,
    workspaceName: vscode.workspace_name || vscode.workspace.split("/").pop() || vscode.workspace,
    file: vscode.file || null,
    line: vscode.line || null,
  });
}

export function resolveChromeUrl(task: MergedTask, targetIndex?: number): string | null {
  const targets = chromeTargetsFromTask(task.config);
  if (typeof targetIndex === "number") return targets[targetIndex]?.url || null;
  return targets[0]?.url || guessUrlFromPorts(task);
}

export async function jumpToChrome(
  task: MergedTask,
  target?: ChromeTarget,
  targetIndex?: number,
): Promise<void> {
  const url = target?.url || resolveChromeUrl(task, targetIndex);
  if (!url) throw new Error("No Chrome URL for this task");

  await invoke("focus_chrome_url", { url });
}

export async function jumpSmart(task: MergedTask): Promise<void> {
  const status = task.effectiveStatus;
  const targets: ("cmux" | "vscode" | "chrome")[] = [];

  switch (status) {
    case "needs_action":
      targets.push("cmux", "vscode", "chrome");
      break;
    case "needs_review":
      targets.push("vscode", "cmux", "chrome");
      break;
    case "executing":
      targets.push("cmux", "vscode", "chrome");
      break;
    case "idle":
      targets.push("cmux", "vscode", "chrome");
      break;
  }

  for (const target of targets) {
    try {
      if (target === "cmux" && (task.cmux || task.config.cmux_workspace_id)) {
        await jumpToCmux(task);
        return;
      }
      if (target === "vscode" && task.config.vscode) {
        await jumpToVscode(task);
        return;
      }
      if (target === "chrome" && (task.config.chrome || task.ports.length > 0)) {
        await jumpToChrome(task);
        return;
      }
    } catch {
      // try next target
    }
  }

  throw new Error("No jump target available for status: " + status);
}

function guessUrlFromPorts(task: MergedTask): string | null {
  if (task.ports.length === 0) return null;
  return "http://localhost:" + task.ports[0];
}
