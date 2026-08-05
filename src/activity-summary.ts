export type ActivityPeriod = "today" | "week";
export type ActivitySource = "cmux" | "codex" | "chrome" | "vscode";
export type ActivityKind = "ai_input" | "ai_reading" | "browser_review" | "code_reading" | "code_editing";

export type DurationBreakdown = { key: string; total_ms: number };
export type TaskActivitySummary = {
  task_id: string | null;
  task_title: string;
  total_ms: number;
  by_source: DurationBreakdown[];
  by_activity: DurationBreakdown[];
  by_confidence: DurationBreakdown[];
};
export type ActivitySummary = {
  total_ms: number;
  tasks: TaskActivitySummary[];
  by_source: DurationBreakdown[];
  by_activity: DurationBreakdown[];
  by_confidence: DurationBreakdown[];
};

export function formatDuration(totalMs: number): string {
  const totalMinutes = Math.floor(totalMs / 60_000);
  if (totalMinutes < 1) return `${Math.max(0, Math.floor(totalMs / 1_000))} 秒`;
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours === 0) return `${minutes} 分`;
  return minutes === 0 ? `${hours} 小时` : `${hours} 小时 ${minutes} 分`;
}

export function rangeForPeriod(period: ActivityPeriod, now = new Date()): { start: number; end: number } {
  const start = new Date(now);
  start.setHours(0, 0, 0, 0);
  if (period === "week") start.setDate(start.getDate() - 6);
  return { start: start.getTime(), end: now.getTime() };
}

export function sourceLabel(source: string): string {
  return ({ cmux: "cmux", codex: "Codex", chrome: "Chrome", vscode: "VS Code" } as Record<string, string>)[source] || source;
}

export function activityLabel(activity: string): string {
  return ({
    ai_input: "AI 输入",
    ai_reading: "AI 阅读",
    browser_review: "浏览器 Review",
    code_reading: "代码阅读",
    code_editing: "代码编辑",
  } as Record<string, string>)[activity] || activity;
}

export function confidenceLabel(confidence: string): string {
  return ({ high: "精确匹配", medium: "跳转关联", low: "未归属" } as Record<string, string>)[confidence] || confidence;
}
