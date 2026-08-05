import { expect, test } from "bun:test";
import { barHeightForContent, barHeightLimits } from "./window-sizing";

test("keeps the current 560px height when content is short", () => {
  expect(barHeightForContent(420, 900)).toBe(560);
});

test("grows to fit task content within the current screen work area", () => {
  expect(barHeightForContent(720, 900)).toBe(720);
  expect(barHeightForContent(1_200, 900)).toBe(876);
});

test("derives safe manual resize limits from a small screen", () => {
  expect(barHeightLimits(300)).toEqual({ min: 276, max: 276 });
});
