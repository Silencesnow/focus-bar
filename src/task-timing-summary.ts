import { formatDuration, sourceLabel, type ActivityPeriod } from "./activity-summary";

export type TaskTimingSegment = {
  kind: "execution" | "interruption";
  duration_ms: number;
};

export type TaskTimingTaskSummary = {
  task_id: string;
  task_title: string;
  source: string;
  execution_ms: number;
  interruption_ms: number;
  execution_count: number;
  interruption_count: number;
  segments: TaskTimingSegment[];
};

export type TaskTimingSummary = {
  task_execution_ms: number;
  actual_execution_ms: number;
  tasks: TaskTimingTaskSummary[];
};

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function periodLabel(period: ActivityPeriod): string {
  return period === "today" ? "今天" : "最近 7 天";
}

export function formatRangeStart(period: ActivityPeriod, startMs: number): string {
  const date = new Date(startMs);
  const time = date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (period === "today") return `今天 ${time} 起`;
  const day = date.toLocaleDateString([], { month: "2-digit", day: "2-digit" });
  return `${day} ${time} 起`;
}

export function executionShare(executionMs: number, interruptionMs: number): number {
  const total = executionMs + interruptionMs;
  if (total <= 0) return 100;
  return (executionMs / total) * 100;
}

function timingBar(task: TaskTimingTaskSummary): string {
  const total = task.segments.reduce((sum, segment) => sum + segment.duration_ms, 0);
  if (total <= 0) {
    return `<div class="timing-bar"><span class="timing-bar-exec" style="width:100%"></span></div>`;
  }
  const cells = task.segments
    .map((segment) => {
      const pct = (segment.duration_ms / total) * 100;
      const cls = segment.kind === "execution" ? "timing-bar-exec" : "timing-bar-interrupt";
      return `<span class="${cls}" style="width:${pct.toFixed(2)}%"></span>`;
    })
    .join("");
  return `<div class="timing-bar" role="img" aria-label="执行 ${formatDuration(task.execution_ms)}，中断 ${formatDuration(task.interruption_ms)}，共 ${task.segments.length} 段">${cells}</div>`;
}

function taskRow(task: TaskTimingTaskSummary): string {
  return `
    <article class="task-stat">
      <div class="task-stat-header">
        <div><h3>${escapeHtml(task.task_title)}</h3><p>${escapeHtml(sourceLabel(task.source))}</p></div>
        <strong>${formatDuration(task.execution_ms)}</strong>
      </div>
      ${timingBar(task)}
      <div class="task-metrics">
        <div class="task-metric"><span>执行</span><strong>${formatDuration(task.execution_ms)}</strong></div>
        <div class="task-metric"><span>中断</span><strong>${formatDuration(task.interruption_ms)}</strong></div>
        <div class="task-metric"><span>轮次</span><strong>${task.execution_count} 执行 · ${task.interruption_count} 中断</strong></div>
      </div>
    </article>`;
}

export function renderTaskTimingSummary(summary: TaskTimingSummary, period: ActivityPeriod): string {
  if (summary.task_execution_ms === 0 && summary.tasks.length === 0) {
    return `<div class="empty-state"><div class="empty-icon">◷</div><h2>还没有任务执行记录</h2><p>当 cmux 或 Codex 任务开始执行后，这里会按任务汇总执行、中断和轮次。</p></div>`;
  }

  const label = periodLabel(period);
  const overview = `
    <section class="overview overview-single">
      <div class="total-card"><span>总运行时长</span><strong>${formatDuration(summary.task_execution_ms)}</strong><small>${label} · AI 一直在干活的时间（各任务执行累加）</small></div>
    </section>`;

  const tasks = summary.tasks.map(taskRow).join("");
  const tasksSection = summary.tasks.length > 0
    ? `<section class="tasks-section"><div class="section-title"><h2>按任务</h2><span>${summary.tasks.length} 项</span></div>${tasks}</section>`
    : "";

  return overview + tasksSection;
}
