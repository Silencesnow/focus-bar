import { expect, test } from "bun:test";
import { resetSettingsViewport } from "./settings-viewport";

test("resets a reused settings window to the top and clears stale focus", () => {
  const viewport = { scrollTop: 480 };
  let blurCount = 0;

  resetSettingsViewport(viewport, { blur: () => { blurCount += 1; } });

  expect(viewport.scrollTop).toBe(0);
  expect(blurCount).toBe(1);
});
