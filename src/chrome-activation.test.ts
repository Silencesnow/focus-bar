import { expect, test } from "bun:test";
import {
  activateChromeTarget,
  isChromeTargetActive,
  type ActiveChromeTargets,
} from "./chrome-activation";

test("keeps the clicked third Chrome target active after a render refresh", () => {
  const activeTargets: ActiveChromeTargets = new Map();

  activateChromeTarget(activeTargets, "task-ling-design-b", 2, "https://example.com/third");

  expect(isChromeTargetActive(
    activeTargets,
    "task-ling-design-b",
    0,
    "https://example.com/first",
  )).toBe(false);
  expect(isChromeTargetActive(
    activeTargets,
    "task-ling-design-b",
    2,
    "https://example.com/third",
  )).toBe(true);
});
