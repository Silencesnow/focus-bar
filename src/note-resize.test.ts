import { expect, test } from "bun:test";
import { noteHeightFromDrag } from "./note-resize";

test("note height follows vertical drag immediately", () => {
  expect(noteHeightFromDrag(92, 34)).toBe(126);
  expect(noteHeightFromDrag(126, -20)).toBe(106);
});

test("note height stays inside its usable range", () => {
  expect(noteHeightFromDrag(70, -100)).toBe(58);
  expect(noteHeightFromDrag(250, 100)).toBe(280);
});
