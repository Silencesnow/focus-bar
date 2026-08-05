import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
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

export function codexThreadUrl(threadId: string): string {
  return `codex://threads/${encodeURIComponent(threadId)}`;
}

export async function jumpToCodex(task: MergedTask): Promise<void> {
  const threadId = task.codex?.id || task.config.codex_thread_id;
  if (!threadId) throw new Error("No Codex thread for this task");
  await openUrl(codexThreadUrl(threadId));
  await invoke("note_codex_thread_opened", { threadId });
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

export type SmartJumpTarget = "cmux" | "vscode" | "chrome";

export function smartJumpTargets(_task: MergedTask): SmartJumpTarget[] {
  return ["cmux", "vscode", "chrome"];
}

export async function jumpSmart(task: MergedTask): Promise<void> {
  if (task.codex || task.config.codex_thread_id) {
    await jumpToCodex(task);
    return;
  }
  const targets = smartJumpTargets(task);

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

  throw new Error("No jump target available for status: " + task.effectiveStatus);
}

function guessUrlFromPorts(task: MergedTask): string | null {
  if (task.ports.length === 0) return null;
  return "http://localhost:" + task.ports[0];
}
