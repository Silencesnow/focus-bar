import { invoke } from "@tauri-apps/api/core";
import type { FocusData, TaskConfig } from "./types";

const FOCUS_FILE = ".focus.json";

export async function readFocusData(): Promise<FocusData> {
  const raw = await invoke<string>("read_home_file", { path: FOCUS_FILE });
  if (!raw.trim()) {
    return { tasks: [] };
  }
  try {
    return JSON.parse(raw) as FocusData;
  } catch {
    return { tasks: [] };
  }
}

export async function writeFocusData(data: FocusData): Promise<void> {
  await invoke("write_home_file", { path: FOCUS_FILE, content: JSON.stringify(data, null, 2) });
}

export function getDefaultFocusData(): FocusData {
  return { tasks: [] };
}

export async function upsertTask(task: TaskConfig): Promise<FocusData> {
  const data = await readFocusData();
  const idx = data.tasks.findIndex((t) => t.id === task.id);
  if (idx >= 0) {
    data.tasks[idx] = task;
  } else {
    data.tasks.push(task);
  }
  await writeFocusData(data);
  return data;
}

export async function updateTaskField(
  taskId: string,
  field: keyof TaskConfig,
  value: unknown
): Promise<FocusData> {
  const data = await readFocusData();
  const task = data.tasks.find((t) => t.id === taskId);
  if (task) {
    (task as unknown as Record<string, unknown>)[field] = value;
    await writeFocusData(data);
  }
  return data;
}

export async function ensureTaskForCmux(
  workspaceId: string,
  workspaceTitle: string,
  cwd: string
): Promise<TaskConfig> {
  const data = await readFocusData();
  let task = data.tasks.find((t) => t.cmux_workspace_id === workspaceId);
  if (!task) {
    task = {
      id: workspaceId.slice(0, 8),
      name: workspaceTitle || cwd.split("/").pop() || "Unnamed",
      name_overridden: false,
      cmux_workspace_id: workspaceId,
      manual_status: null,
      note: "",
    };
    data.tasks.push(task);
    await writeFocusData(data);
  }
  return task;
}
