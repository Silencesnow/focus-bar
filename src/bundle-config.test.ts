import { expect, test } from "bun:test";

test("the local macOS bundle receives a complete ad-hoc signature", async () => {
  const config = await Bun.file(
    new URL("../src-tauri/tauri.conf.json", import.meta.url),
  ).json() as { bundle: { macOS?: { signingIdentity?: string } } };

  expect(config.bundle.macOS?.signingIdentity).toBe("-");
});
