import { expect, test } from "bun:test";

test("hidden settings sections cannot be made visible by component display rules", async () => {
  const css = await Bun.file(new URL("./settings.css", import.meta.url)).text();

  expect(css).toMatch(/\[hidden\]\s*\{[^}]*display:\s*none\s*!important;/s);
});
