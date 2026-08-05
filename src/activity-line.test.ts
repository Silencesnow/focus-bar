import { expect, test } from "bun:test";

test("task cards render a single-line activity summary", async () => {
  const [main, css] = await Promise.all([
    Bun.file(new URL("./main.ts", import.meta.url)).text(),
    Bun.file(new URL("./styles.css", import.meta.url)).text(),
  ]);

  expect(main).toContain('class="card-activity"');
  expect(main).toContain("escapeHtml(activityText)");
  expect(css).toMatch(/\.card-activity\s*\{[^}]*white-space:\s*nowrap;[^}]*overflow:\s*hidden;[^}]*text-overflow:\s*ellipsis;/s);
});

test("idle task cards omit the redundant status label", async () => {
  const main = await Bun.file(new URL("./main.ts", import.meta.url)).text();

  expect(main).toContain('task.effectiveStatus === "idle" ? "" : meta.label');
  expect(main).toContain('const activity = activityText');
  expect(main).toContain("    activity,");
});

test("task cards use a stable Tab icon instead of a status square", async () => {
  const [main, css] = await Promise.all([
    Bun.file(new URL("./main.ts", import.meta.url)).text(),
    Bun.file(new URL("./styles.css", import.meta.url)).text(),
  ]);

  expect(main).toContain("tabIconForTask(task.config.tab_icon, task.title)");
  expect(main).toContain('class="tab-icon"');
  expect(main).not.toContain('class="status-dot">${meta.emoji}');
  expect(css).toMatch(/\.tab-icon\s*\{[^}]*border-color:\s*var\(--status-color\);/s);
});

test("tool buttons use bundled application icons", async () => {
  const main = await Bun.file(new URL("./main.ts", import.meta.url)).text();

  expect(main).toContain('import cmuxIcon from "./assets/tool-icons/cmux.png"');
  expect(main).toContain('import vscodeIcon from "./assets/tool-icons/vscode.png"');
  expect(main).toContain('import chromeIcon from "./assets/tool-icons/chrome.png"');
  expect(main).not.toContain('aria-label="跳转 cmux">📟');
  expect(main).not.toContain('aria-label="跳转 VS Code">📝');
  expect(main).not.toContain('<span aria-hidden="true">🌐</span>');
});

test("the Codex tool button uses a prominent application badge", async () => {
  const [main, css] = await Promise.all([
    Bun.file(new URL("./main.ts", import.meta.url)).text(),
    Bun.file(new URL("./styles.css", import.meta.url)).text(),
  ]);

  expect(main).toContain('class="tool-app-icon codex-tool-icon"');
  expect(css).toMatch(/\.codex-tool-icon\s*\{[^}]*width:\s*18px;[^}]*height:\s*18px;[^}]*display:\s*inline-flex;[^}]*background:/s);
});

test("action and review tasks use one pending attention interaction", async () => {
  const main = await Bun.file(new URL("./main.ts", import.meta.url)).text();

  expect(main).toContain("function isPendingAttention(status: TaskStatus)");
  expect(main).toContain("const pending = tasks.find((task) => isPendingAttention(task.effectiveStatus));");
  expect(main).toContain('return `🔴 处理「${pending.title}」`;');
  expect(main).toContain("mergedTasks.some((task) => isPendingAttention(task.effectiveStatus))");
  expect(main).toContain("mergedTasks.find((item) => isPendingAttention(item.effectiveStatus))");
  expect(main).not.toContain("🟡 检查");
  expect(main).toContain('{ value: "needs_review", label: "🔴 待处理" }');
});
