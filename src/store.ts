import { invoke } from "@tauri-apps/api/core";
import type { CmuxWorkspace, CodexThread, FocusData, TaskConfig } from "./types";

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

export function setGlobalNote(data: FocusData, note: string): FocusData {
  return { ...data, global_note: note };
}

function directoryKey(value: string | null | undefined): string | null {
  if (!value?.trim()) return null;
  const normalized = value.trim().replace(/\/+$/, "");
  return normalized || "/";
}

function savedDirectory(task: TaskConfig): string | null {
  return directoryKey(task.cmux_directory || task.vscode?.workspace);
}

function newTaskId(workspaceId: string, tasks: TaskConfig[]): string {
  const base = workspaceId.slice(0, 8) || workspaceId;
  if (!tasks.some((task) => task.id === base)) return base;
  let suffix = 2;
  while (tasks.some((task) => task.id === `${base}-${suffix}`)) suffix += 1;
  return `${base}-${suffix}`;
}

export function reconcileWorkspaceConfigs(
  workspaces: CmuxWorkspace[],
  data: FocusData,
): { data: FocusData; workspaceConfigs: TaskConfig[]; changed: boolean } {
  const tasks = data.tasks.map((task) => ({ ...task }));
  const activeWorkspaceIds = new Set(workspaces.map((workspace) => workspace.id));
  const claimedTaskIds = new Set<string>();
  const workspaceConfigs: TaskConfig[] = [];
  let changed = false;

  for (const workspace of workspaces) {
    let config = tasks.find((task) => task.cmux_workspace_id === workspace.id);
    const workspaceDirectory = directoryKey(workspace.current_directory);

    if (!config && workspaceDirectory) {
      const candidates = tasks.filter((task) => (
        !claimedTaskIds.has(task.id)
        && !activeWorkspaceIds.has(task.cmux_workspace_id || "")
        && savedDirectory(task) === workspaceDirectory
      ));
      if (candidates.length === 1) {
        config = candidates[0];
        config.cmux_workspace_id = workspace.id;
        config.manual_status = null;
        config.manual_status_context_id = null;
        config.last_viewed_at = null;
        changed = true;
      }
    }

    if (!config) {
      config = {
        id: newTaskId(workspace.id, tasks),
        name: workspace.title || workspace.current_directory.split("/").pop() || "Unnamed",
        name_overridden: false,
        cmux_workspace_id: workspace.id,
        manual_status: null,
        note: "",
      };
      tasks.push(config);
      changed = true;
    }

    if (config.manual_status && config.manual_status_context_id !== workspace.id) {
      config.manual_status = null;
      config.manual_status_context_id = null;
      changed = true;
    }

    if (workspaceDirectory && config.cmux_directory !== workspaceDirectory) {
      config.cmux_directory = workspaceDirectory;
      changed = true;
    }
    claimedTaskIds.add(config.id);
    workspaceConfigs.push(config);
  }

  return { data: { ...data, tasks }, workspaceConfigs, changed };
}

export function reconcileCodexConfigs(
  threads: CodexThread[],
  data: FocusData,
): { data: FocusData; threadConfigs: TaskConfig[]; changed: boolean } {
  const tasks = data.tasks.map((task) => ({ ...task }));
  const threadConfigs: TaskConfig[] = [];
  let changed = false;

  for (const thread of threads) {
    let config = tasks.find((task) => task.codex_thread_id === thread.id);
    if (!config) {
      config = {
        id: newTaskId(`codex-${thread.id}`, tasks),
        name: thread.title || thread.cwd.split("/").pop() || "Codex task",
        name_overridden: false,
        codex_thread_id: thread.id,
        codex_directory: directoryKey(thread.cwd) || thread.cwd,
        manual_status: null,
        note: "",
      };
      tasks.push(config);
      changed = true;
    }
    if (config.manual_status && config.manual_status_context_id !== thread.id) {
      config.manual_status = null;
      config.manual_status_context_id = null;
      changed = true;
    }
    const directory = directoryKey(thread.cwd);
    if (directory && config.codex_directory !== directory) {
      config.codex_directory = directory;
      changed = true;
    }
    threadConfigs.push(config);
  }

  return { data: { ...data, tasks }, threadConfigs, changed };
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

export function recordTaskViewed(data: FocusData, taskId: string, viewedAt: string): boolean {
  const task = data.tasks.find((item) => item.id === taskId);
  if (!task) return false;
  task.last_viewed_at = viewedAt;
  return true;
}

export async function markTaskViewed(
  taskId: string,
  viewedAt = new Date().toISOString(),
): Promise<void> {
  const data = await readFocusData();
  if (recordTaskViewed(data, taskId, viewedAt)) await writeFocusData(data);
}

export function recordTaskCollapsed(
  data: FocusData,
  taskId: string,
  collapsedAt: string,
): boolean {
  const task = data.tasks.find((item) => item.id === taskId);
  if (!task) return false;
  task.manually_collapsed_at = collapsedAt;
  return true;
}

export async function markTaskCollapsed(
  taskId: string,
  collapsedAt = new Date().toISOString(),
): Promise<void> {
  const data = await readFocusData();
  if (recordTaskCollapsed(data, taskId, collapsedAt)) await writeFocusData(data);
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
