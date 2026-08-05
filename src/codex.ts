import { invoke } from "@tauri-apps/api/core";
import type { CodexSnapshot } from "./types";

export async function fetchCodexSnapshot(): Promise<CodexSnapshot> {
  return invoke<CodexSnapshot>("fetch_codex_snapshot");
}
