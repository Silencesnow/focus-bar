import { expect, test } from "bun:test";
import { applyNativeHoverAtCursor } from "./inactive-hover";

class FakeClassList {
  values = new Set<string>();

  add(value: string) {
    this.values.add(value);
  }

  remove(value: string) {
    this.values.delete(value);
  }
}

function fakeElement() {
  return { classList: new FakeClassList() };
}

test("maps physical cursor coordinates to every hovered DOM target", () => {
  const previous = fakeElement();
  previous.classList.add("is-native-hovered");
  const card = fakeElement();
  const tool = fakeElement();
  const inactiveToggle = fakeElement();
  const hit = {
    closest(selector: string) {
      if (selector === ".task-card") return card;
      if (selector === ".tool-button") return tool;
      if (selector === ".inactive-task-toggle") return inactiveToggle;
      return null;
    },
  };

  const next = applyNativeHoverAtCursor(
    [previous],
    { x: 2484, y: 98 },
    { x: 1320, y: 64 },
    2,
    (x, y) => {
      expect({ x, y }).toEqual({ x: 582, y: 17 });
      return hit;
    },
  );

  expect(previous.classList.values.has("is-native-hovered")).toBe(false);
  expect(card.classList.values.has("is-native-hovered")).toBe(true);
  expect(tool.classList.values.has("is-native-hovered")).toBe(true);
  expect(inactiveToggle.classList.values.has("is-native-hovered")).toBe(true);
  expect(next).toEqual([card, tool, inactiveToggle]);
});

test("native hover classes use the same visual rules as CSS hover", async () => {
  const css = await Bun.file(new URL("./styles.css", import.meta.url)).text();
  const selectors = [
    "#settings-button",
    ".task-card",
    ".tool-button",
    ".inactive-task-toggle",
    "#suggestion.clickable",
    ".ctx-item",
  ];

  for (const selector of selectors) {
    const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    expect(css).toMatch(new RegExp(`${escaped}:hover,\\s*${escaped}\\.is-native-hovered`));
  }
});
