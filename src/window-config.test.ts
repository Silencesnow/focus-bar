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

test("the focus bar uses the taller activity-card height everywhere", async () => {
  const [config, main, css] = await Promise.all([
    Bun.file(new URL("../src-tauri/tauri.conf.json", import.meta.url)).json() as Promise<{
      app: { windows: Array<{ label: string; height: number }> };
    }>,
    Bun.file(new URL("./main.ts", import.meta.url)).text(),
    Bun.file(new URL("./styles.css", import.meta.url)).text(),
  ]);
  const mainWindow = config.app.windows.find((window) => window.label === "main");

  expect(mainWindow?.height).toBe(156);
  expect(main).toContain("const BAR_HEIGHT = 156;");
  expect(css).toMatch(/#bar\s*\{[^}]*height:\s*156px;/s);
});
