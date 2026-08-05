import { expect, test } from "bun:test";

test("the inactive focus bar accepts the first mouse interaction", async () => {
  const config = await Bun.file(
    new URL("../src-tauri/tauri.conf.json", import.meta.url),
  ).json() as { app: { windows: Array<{ label: string; acceptFirstMouse?: boolean }> } };
  const mainWindow = config.app.windows.find((window) => window.label === "main");
  const settingsWindow = config.app.windows.find((window) => window.label === "settings");

  expect(mainWindow?.acceptFirstMouse).toBe(true);
  expect(settingsWindow?.acceptFirstMouse).not.toBe(true);
});
