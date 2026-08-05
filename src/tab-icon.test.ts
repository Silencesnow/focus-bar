import { expect, test } from "bun:test";
import { normalizeTabIcon, tabIconForTask } from "./tab-icon";

test("normalizes one emoji or two letters", () => {
  expect(normalizeTabIcon(" 🎨 ")).toBe("🎨");
  expect(normalizeTabIcon("fe")).toBe("FE");
});

test("keeps a composed emoji intact", () => {
  expect(normalizeTabIcon("👨‍💻extra")).toBe("👨‍💻");
});

test("derives compact tab initials when no icon is configured", () => {
  expect(tabIconForTask(undefined, "ling-design-B 样式迁移")).toBe("LD");
  expect(tabIconForTask("", "矢量性能优化")).toBe("矢");
});

test("uses the configured tab icon before the title fallback", () => {
  expect(tabIconForTask("qa", "ling-design")).toBe("QA");
});
