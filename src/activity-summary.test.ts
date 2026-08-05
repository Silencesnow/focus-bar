import { expect, test } from "bun:test";
import { activityLabel, confidenceLabel, formatDuration, rangeForPeriod, sourceLabel } from "./activity-summary";

test("formats tracked duration without implying false precision", () => {
  expect(formatDuration(42_000)).toBe("42 秒");
  expect(formatDuration(5_700_000)).toBe("1 小时 35 分");
});

test("today and seven-day ranges start at local midnight", () => {
  const now = new Date(2026, 6, 21, 15, 30, 0);
  expect(new Date(rangeForPeriod("today", now).start)).toEqual(new Date(2026, 6, 21));
  expect(new Date(rangeForPeriod("week", now).start)).toEqual(new Date(2026, 6, 15));
});

test("presents sources and activities in concise Chinese", () => {
  expect(sourceLabel("cmux")).toBe("cmux");
  expect(sourceLabel("codex")).toBe("Codex");
  expect(activityLabel("browser_review")).toBe("浏览器 Review");
  expect(activityLabel("code_editing")).toBe("代码编辑");
  expect(confidenceLabel("medium")).toBe("跳转关联");
});
