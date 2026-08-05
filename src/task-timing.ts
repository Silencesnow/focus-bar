import { invoke } from "@tauri-apps/api/core";
import type { MergedTask, TaskStatus } from "./types";

export interface TaskStatusSample {
  task_id: string;
  task_title: string;
  source: "cmux" | "codex";
  status: TaskStatus;
}

function sampleFor(task: MergedTask, source: "cmux" | "codex"): TaskStatusSample | null {
  const taskId = task.config.id.trim();
  if (!taskId) return null;
  return {
    task_id: taskId,
    task_title: task.title,
    source: task.source ?? source,
    status: task.effectiveStatus,
  };
}

export function taskStatusSamples(
  cmuxTasks: MergedTask[],
  codexTasks: MergedTask[],
): TaskStatusSample[] {
  const samples: TaskStatusSample[] = [];
  for (const task of cmuxTasks) {
    const sample = sampleFor(task, "cmux");
    if (sample) samples.push(sample);
  }
  for (const task of codexTasks) {
    const sample = sampleFor(task, "codex");
    if (sample) samples.push(sample);
  }
  return samples;
}

export async function recordTaskStatusSnapshot(
  samples: TaskStatusSample[],
  observedAt: number,
): Promise<void> {
  await invoke("record_task_status_snapshot", { tasks: samples, observedAt });
}
