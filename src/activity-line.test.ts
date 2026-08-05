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
