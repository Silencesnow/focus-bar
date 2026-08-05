import { taskDisplayName } from "./view-model";
import type { CmuxWorkspace, CodexThread, TaskConfig } from "./types";

export interface SettingsTask {
  source: "cmux" | "codex";
  title: string;
  runtimeTitle: string;
  directory: string;
  config: TaskConfig;
}

export function buildSettingsTasks(
  workspaces: CmuxWorkspace[],
  threads: CodexThread[],
  configs: TaskConfig[],
): SettingsTask[] {
  const cmuxTasks = workspaces.flatMap((workspace) => {
    const config = configs.find((item) => item.cmux_workspace_id === workspace.id);
    if (!config) return [];
    return [{
      source: "cmux" as const,
      title: taskDisplayName(workspace, config),
      runtimeTitle: workspace.title.trim(),
      directory: workspace.current_directory,
      config,
    }];
  });
  const codexTasks = threads.flatMap((thread) => {
    const config = configs.find((item) => item.codex_thread_id === thread.id);
    if (!config) return [];
    return [{
      source: "codex" as const,
      title: config.name_overridden && config.name.trim()
        ? config.name.trim()
        : thread.title.trim() || config.name.trim() || "Codex task",
      runtimeTitle: thread.title.trim(),
      directory: thread.cwd,
      config,
    }];
  });
  return [...cmuxTasks, ...codexTasks];
}
