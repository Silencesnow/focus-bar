import { expect, test } from "bun:test";
import { createGlobalNoteAutosave } from "./global-note";
import { setGlobalNote } from "./store";
import type { FocusData } from "./types";

test("setting the global note preserves task configuration", () => {
  const data: FocusData = {
    tasks: [{ id: "task-1", name: "Task", note: "task note" }],
  };

  const updated = setGlobalNote(data, "- review MR\n- run tests");

  expect(updated.global_note).toBe("- review MR\n- run tests");
  expect(updated.tasks).toEqual(data.tasks);
  expect(data.global_note).toBeUndefined();
});

test("autosave collapses rapid edits and flushes the latest note", async () => {
  const saved: string[] = [];
  const states: string[] = [];
  const autosave = createGlobalNoteAutosave(async (note) => {
    saved.push(note);
  }, 300);

  autosave.schedule("first", (state) => states.push(state));
  autosave.schedule("latest", (state) => states.push(state));
  await autosave.flush();

  expect(saved).toEqual(["latest"]);
  expect(states).toEqual(["pending", "pending", "saving", "saved"]);
});
