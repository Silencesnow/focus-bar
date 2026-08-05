import { describe, expect, test } from "bun:test";
import { executionShare, formatRangeStart, renderTaskTimingSummary, type TaskTimingSummary } from "./task-timing-summary";

function summary(overrides: Partial<TaskTimingSummary> = {}): TaskTimingSummary {
  return {
    task_execution_ms: 0,
    actual_execution_ms: 0,
    tasks: [],
    ...overrides,
  };
}

describe("renderTaskTimingSummary", () => {
  test("shows the empty state when nothing ran", () => {
    const html = renderTaskTimingSummary(summary(), "today");
    expect(html).toContain("empty-state");
    expect(html).not.toContain("total-card");
  });

  test("renders the total running time", () => {
    const html = renderTaskTimingSummary(
      summary({ task_execution_ms: 90 * 60_000, actual_execution_ms: 60 * 60_000 }),
      "today",
    );
    expect(html).toContain("总运行时长");
    expect(html).not.toContain("实际运行覆盖");
    expect(html).toContain("1 小时 30 分");
  });

  test("renders per-task execution, interruption, and round counts", () => {
    const html = renderTaskTimingSummary(
      summary({
        task_execution_ms: 25 * 60_000,
        actual_execution_ms: 25 * 60_000,
        tasks: [
          {
            task_id: "task-1",
            task_title: "重构状态机",
            source: "cmux",
            execution_ms: 25 * 60_000,
            interruption_ms: 5 * 60_000,
            execution_count: 3,
            interruption_count: 2,
            segments: [
              { kind: "execution", duration_ms: 10 * 60_000 },
              { kind: "interruption", duration_ms: 3 * 60_000 },
              { kind: "execution", duration_ms: 10 * 60_000 },
              { kind: "interruption", duration_ms: 2 * 60_000 },
              { kind: "execution", duration_ms: 5 * 60_000 },
            ],
          },
        ],
      }),
      "today",
    );
    expect(html).toContain("重构状态机");
    expect(html).toContain("25 分");
    expect(html).toContain("5 分");
    expect(html).toContain("3");
    expect(html).toContain("2");
    expect(html).toContain("执行");
    expect(html).toContain("中断");
    expect(html).toContain("轮次");
    expect(html).toContain("timing-bar");
    expect(html.match(/timing-bar-exec/g)?.length).toBe(3);
    expect(html.match(/timing-bar-interrupt/g)?.length).toBe(2);
    const firstExec = html.indexOf("timing-bar-exec");
    const firstInterrupt = html.indexOf("timing-bar-interrupt");
    const secondExec = html.indexOf("timing-bar-exec", firstExec + 1);
    expect(firstExec).toBeLessThan(firstInterrupt);
    expect(firstInterrupt).toBeLessThan(secondExec);
  });

  test("executionShare splits execution vs interruption proportionally", () => {
    expect(executionShare(0, 0)).toBe(100);
    expect(executionShare(30, 10)).toBe(75);
    expect(executionShare(0, 40)).toBe(0);
    expect(executionShare(40, 0)).toBe(100);
  });

  test("labels today and week periods", () => {
    const today = renderTaskTimingSummary(summary({ task_execution_ms: 1_000 }), "today");
    const week = renderTaskTimingSummary(summary({ task_execution_ms: 1_000 }), "week");
    expect(today).toContain("今天");
    expect(week).toContain("最近 7 天");
  });

  test("formatRangeStart shows a time for today and a date for week", () => {
    const start = new Date(2026, 7, 5, 9, 5, 0).getTime();
    expect(formatRangeStart("today", start)).toContain("今天");
    expect(formatRangeStart("today", start)).toContain("起");
    expect(formatRangeStart("week", start)).toContain("起");
    expect(formatRangeStart("week", start)).not.toContain("今天");
  });

  test("escapes task titles", () => {
    const html = renderTaskTimingSummary(
      summary({
        task_execution_ms: 1_000,
        actual_execution_ms: 1_000,
        tasks: [
          {
            task_id: "x",
            task_title: "<script>",
            source: "codex",
            execution_ms: 1_000,
            interruption_ms: 0,
            execution_count: 1,
            interruption_count: 0,
            segments: [{ kind: "execution", duration_ms: 1_000 }],
          },
        ],
      }),
      "today",
    );
    expect(html).not.toContain("<script>");
    expect(html).toContain("&lt;script&gt;");
  });
});
