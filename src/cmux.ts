import { invoke } from "@tauri-apps/api/core";
import type { CmuxSnapshot } from "./types";

export async function fetchAll(): Promise<CmuxSnapshot> {
  return invoke<CmuxSnapshot>("fetch_cmux_snapshot");
}

export async function startWatcher(): Promise<void> {
  await invoke("start_cmux_watcher");
}

export async function focusWorkspace(
  workspaceRef: string,
  workspaceId: string,
  windowId: string,
): Promise<void> {
  await invoke("focus_cmux_workspace", {
    workspaceRef,
    workspaceId,
    windowId,
  });
}
