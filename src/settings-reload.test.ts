import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

test("opening settings reloads cmux workspaces before selecting the requested task", () => {
  const source = readFileSync(new URL("./settings.ts", import.meta.url), "utf8");
  const openHandler = source.match(/await listen<\{ taskId\?: string \}>\([\s\S]*?\n  \}\);/)?.[0] || "";

  expect(openHandler).toContain("await loadTasks()");
});
