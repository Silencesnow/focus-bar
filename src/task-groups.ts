import type { MergedTask } from "./types";

export interface TaskGroups {
  current: MergedTask[];
  inactive: MergedTask[];
}

export interface InactiveTaskToggle {
  count: number;
  label: string;
  expanded: boolean;
}

export interface TaskListModel extends TaskGroups {
  visible: MergedTask[];
  inactiveToggle: InactiveTaskToggle | null;
}

export function groupTasksByToday(
  tasks: MergedTask[],
  now = Date.now(),
): TaskGroups {
  const start = new Date(now);
  start.setHours(0, 0, 0, 0);
  const startAt = start.getTime();
  const current: MergedTask[] = [];
  const inactive: MergedTask[] = [];

  for (const task of tasks) {
    const activityAt = task.activityAt ? Date.parse(task.activityAt) : Number.NaN;
    const collapsedAt = task.config.manually_collapsed_at
      ? Date.parse(task.config.manually_collapsed_at)
      : Number.NaN;
    const manuallyInactive = Number.isFinite(collapsedAt)
      && (!Number.isFinite(activityAt) || activityAt <= collapsedAt);
    const isInactive = task.effectiveStatus === "idle"
      && (manuallyInactive || !Number.isFinite(activityAt) || activityAt < startAt);
    (isInactive ? inactive : current).push(task);
  }

  return { current, inactive };
}

export function taskListModel(
  tasks: MergedTask[],
  inactiveExpanded: boolean,
  now = Date.now(),
): TaskListModel {
  const groups = groupTasksByToday(tasks, now);
  return {
    ...groups,
    visible: inactiveExpanded
      ? [...groups.current, ...groups.inactive]
      : groups.current,
    inactiveToggle: groups.inactive.length > 0
      ? {
          count: groups.inactive.length,
          label: `较早任务（${groups.inactive.length}）`,
          expanded: inactiveExpanded,
        }
      : null,
  };
}

export function findTaskByConfigId(
  tasks: MergedTask[],
  taskId: string,
): MergedTask | null {
  return tasks.find((task) => task.config.id === taskId) || null;
}
