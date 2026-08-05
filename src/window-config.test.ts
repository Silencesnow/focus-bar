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

test("the focus bar uses a vertical left-side layout", async () => {
  const [config, main, css] = await Promise.all([
    Bun.file(new URL("../src-tauri/tauri.conf.json", import.meta.url)).json() as Promise<{
      app: { windows: Array<{ label: string; width: number; height: number; resizable?: boolean }> };
    }>,
    Bun.file(new URL("./main.ts", import.meta.url)).text(),
    Bun.file(new URL("./styles.css", import.meta.url)).text(),
  ]);
  const mainWindow = config.app.windows.find((window) => window.label === "main");

  expect(mainWindow?.width).toBe(200);
  expect(mainWindow?.height).toBe(560);
  expect(mainWindow?.resizable).toBe(true);
  expect(main).toContain("const BAR_WIDTH = 200;");
  expect(main).toContain("const BAR_HEIGHT = 560;");
  expect(main).toContain("async function positionWindowLeftCenter(height = BAR_HEIGHT)");
  expect(main).toContain('startResizeDragging("South")');
  expect(main).toContain("setMaxSize");
  expect(main).toContain("window.innerHeight - bounds.height - 8");
  expect(main).not.toContain("MENU_HEIGHT");
  expect(css).toMatch(/#bar\s*\{[^}]*height:\s*100%;/s);
  expect(css).toMatch(/#resize-handle\s*\{[^}]*cursor:\s*ns-resize;/s);
  expect(css).toMatch(/#task-container\s*\{[^}]*flex-direction:\s*column;/s);
  expect(css).toMatch(/#task-container\s*\{[^}]*overflow-y:\s*auto;/s);
  expect(css).toMatch(/\.task-card\s*\{[^}]*width:\s*100%;/s);
  expect(css).toMatch(/\.task-card\s*\{[^}]*max-width:\s*none;/s);
  expect(css).toMatch(/\.card-tools\s*\{[^}]*flex-wrap:\s*wrap;/s);
  expect(css).toMatch(/\.card-tools\s*\{[^}]*overflow-x:\s*visible;/s);
  expect(css).toMatch(/#context-menu\s*\{[^}]*width:\s*calc\(100% - 16px\);/s);
  expect(css).toMatch(/#context-menu\s*\{[^}]*min-width:\s*0;/s);
});

test("the bar includes a custom-resizable five-line global note below its task summary", async () => {
  const [html, main, css] = await Promise.all([
    Bun.file(new URL("../index.html", import.meta.url)).text(),
    Bun.file(new URL("./main.ts", import.meta.url)).text(),
    Bun.file(new URL("./styles.css", import.meta.url)).text(),
  ]);
  const suggestionIndex = html.indexOf('id="suggestion"');
  const noteIndex = html.indexOf('id="global-note-panel"');
  const resizeIndex = html.indexOf('id="resize-handle"');

  expect(html).toContain('<textarea id="global-note" rows="5"');
  expect(html).toContain('id="global-note-resizer"');
  expect(suggestionIndex).toBeLessThan(noteIndex);
  expect(noteIndex).toBeLessThan(resizeIndex);
  expect(main).toContain('document.getElementById("global-note-panel")');
  expect(main).toContain("notePanel?.offsetHeight");
  expect(main).toContain('target.closest("#global-note-panel")');
  expect(css).toMatch(/#global-note-panel\s*\{[^}]*flex:\s*0 0 auto;/s);
  expect(css).toMatch(/#global-note\s*\{[^}]*resize:\s*none;/s);
  expect(css).toMatch(/#global-note\s*\{[^}]*max-height:\s*280px;/s);
  expect(css).toMatch(/#global-note-resizer\s*\{[^}]*width:\s*24px;[^}]*height:\s*24px;[^}]*cursor:\s*ns-resize;/s);
  expect(main).toContain("startGlobalNoteResize");
});

test("the stats button asks the backend to show or create its window", async () => {
  const [main, backend] = await Promise.all([
    Bun.file(new URL("./main.ts", import.meta.url)).text(),
    Bun.file(new URL("../src-tauri/src/lib.rs", import.meta.url)).text(),
  ]);

  expect(main).toContain('invoke("open_activity_stats_window")');
  expect(backend).toContain("fn open_activity_stats_window(app: tauri::AppHandle)");
  expect(backend).toContain("app.get_webview_window(\"stats\")");
  expect(backend).toContain("tauri::WebviewWindowBuilder::new");
});
