import { invoke } from "@tauri-apps/api/core";
import type { MergedTask } from "./types";
import { focusWorkspace } from "./cmux";

async function shellOutput(cmd: string, args: string[]): Promise<string> {
  return invoke<string>("shell_output", { cmd, args });
}

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

  const folderName = vscode.workspace.split("/").pop() || vscode.workspace;

  const applescript = [
    'tell application "System Events"',
    '  tell process "Code"',
    '    set frontmost to true',
    '    set matched to false',
    '    repeat with w in windows',
    '      if name of w contains "' + folderName + '" then',
    '        perform action "AXRaise" of w',
    '        set matched to true',
    '        exit repeat',
    '      end if',
    '    end repeat',
    '  end tell',
    'end tell',
  ].join("\n");

  await shellOutput("osascript", ["-e", applescript]);

  const gotoParts: string[] = ["--goto"];
  if (vscode.file) {
    let target = vscode.workspace + "/" + vscode.file;
    if (vscode.line && vscode.line > 0) {
      target += ":" + vscode.line;
    }
    gotoParts.push(target);
  } else {
    gotoParts.push(vscode.workspace);
  }

  await shellOutput("code", gotoParts);
}

export async function jumpToChrome(task: MergedTask): Promise<void> {
  const url = task.config.chrome?.url || guessUrlFromPorts(task);
  if (!url) throw new Error("No Chrome URL for this task");

  const applescript = [
    'tell application "Google Chrome"',
    "  set found to false",
    "  repeat with w in windows",
    "    repeat with t in tabs of w",
    '      if URL of t contains "' + extractHost(url) + '" then',
    "        set active tab index of w to index of t",
    "        set index of w to 1",
    "        set found to true",
    "        exit repeat",
    "      end if",
    "    end repeat",
    "    if found then exit repeat",
    "  end repeat",
    "  if not found then",
    '    open location "' + url + '"',
    "  end if",
    "  activate",
    "end tell",
  ].join("\n");

  await shellOutput("osascript", ["-e", applescript]);
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

function extractHost(url: string): string {
  try {
    const parsed = new URL(url);
    return parsed.host;
  } catch {
    return url;
  }
}
