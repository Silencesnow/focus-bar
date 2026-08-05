# cmux Focus Bar MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the standalone Tauri Focus Bar reliably query live cmux workspaces, derive four attention states, refresh from cmux events, and jump to the correct workspace with actionable errors.

**Architecture:** A focused Rust `CmuxRuntime` owns executable/socket discovery, typed CLI execution, snapshots, event watching, and jump actions. The TypeScript frontend consumes structured snapshots, computes status with a pure reducer, preserves the last good state on source failures, and renders only currently open cmux workspaces.

**Tech Stack:** Tauri 2, Rust 2021, Tokio, Serde, TypeScript 5.6, Bun test runner, Vite 6, macOS cmux 0.64.x.

## Global Constraints

- cmux socket access is `allowAll` for the single-user Mac MVP.
- The app diagnoses but does not silently edit `~/.config/cmux/cmux.json`.
- Rendered states are exactly `executing`, `needs_action`, `needs_review`, and `idle`.
- Active cmux workspaces are the rendered source of truth; absent historical records are preserved but hidden.
- Command timeouts are bounded and source errors are never converted to an empty successful snapshot.
- Current directory has no Git metadata, so each task ends with a test checkpoint instead of a commit.

## File Structure

- Create `src/status.ts`: pure automatic/manual status derivation and legacy status normalization.
- Create `src/status.test.ts`: Bun table tests for status priority and timestamp behavior.
- Create `src-tauri/src/cmux_runtime.rs`: CLI/socket discovery, structured snapshots, watcher, error mapping, and jump command.
- Modify `src-tauri/src/lib.rs`: register runtime commands and remove the embedded Python fetch script.
- Modify `src/types.ts`: four-state model, source-health types, and structured snapshot contracts.
- Modify `src/cmux.ts`: thin Tauri IPC adapter; no error swallowing.
- Modify `src/jump.ts`: route cmux jumps through the backend transaction.
- Modify `src/main.ts`: snapshot merge, source health, refresh serialization, event subscription, and active-workspace-only rendering.
- Modify `index.html`: source-health container.
- Modify `src/styles.css`: source-health/stale styling and usable temporary context-menu window layout.
- Modify `package.json`: add `test` and `check` scripts.
- Modify `README.md`: prerequisites, `allowAll` setup, development, tests, and error meanings.
- Replace `test_status.py` and `test_e2e.py` with `scripts/test_cmux_live.py`: opt-in, non-destructive live integration checks by default.

---

### Task 1: Pure four-state reducer

**Files:**
- Create: `src/status.ts`
- Create: `src/status.test.ts`
- Modify: `src/types.ts`
- Modify: `package.json`

**Interfaces:**
- Consumes: `CmuxNotification`, ISO timestamp strings, and optional stored manual status.
- Produces: `deriveTaskStatus(input: StatusInput): TaskStatus`, `normalizeManualStatus(value: unknown): TaskStatus | null`, and `statusReason(input: StatusInput): string | null`.

- [ ] **Step 1: Define the four-state and snapshot types**

Replace the old `TaskStatus` union in `src/types.ts` with:

```ts
export type TaskStatus = "needs_action" | "needs_review" | "executing" | "idle";
export type LegacyTaskStatus = "blocked" | "review" | "verifying" | "done";

export type CmuxSourceErrorCode =
  | "CLI_NOT_FOUND"
  | "CMUX_NOT_RUNNING"
  | "ACCESS_DENIED"
  | "TIMEOUT"
  | "INVALID_RESPONSE"
  | "WATCHER_DISCONNECTED";

export type CmuxSourceState =
  | { status: "ready"; cli_path: string; socket_path: string | null }
  | { status: "error"; code: CmuxSourceErrorCode; message: string; detail: string | null };

export interface CmuxSnapshot {
  source: CmuxSourceState;
  workspaces: CmuxWorkspace[];
  notifications: CmuxNotification[];
  fetched_at: number;
}
```

Update `TaskConfig.manual_status` to accept `TaskStatus | LegacyTaskStatus | null` during migration, while `MergedTask.effectiveStatus` remains `TaskStatus`.

- [ ] **Step 2: Write failing reducer tests**

Create `src/status.test.ts` with Bun tests for:

```ts
import { describe, expect, test } from "bun:test";
import { deriveTaskStatus, normalizeManualStatus } from "./status";

const notification = (overrides: Record<string, unknown> = {}) => ({
  id: "n1", workspace_id: "w1", title: "Agent", subtitle: "", body: "",
  is_read: false, created_at: "2026-07-10T10:00:00Z", tab_title: null, ...overrides,
});

describe("deriveTaskStatus", () => {
  test("needs action outranks review", () => {
    expect(deriveTaskStatus({ manualStatus: null, latestSubmittedAt: null, notifications: [
      notification({ id: "done", subtitle: "Completed" }),
      notification({ id: "wait", subtitle: "Waiting for input", created_at: "2026-07-10T09:00:00Z" }),
    ]})).toBe("needs_action");
  });
  test("completed unread work needs review", () => {
    expect(deriveTaskStatus({ manualStatus: null, latestSubmittedAt: null,
      notifications: [notification({ body: "DONE successfully" })] })).toBe("needs_review");
  });
  test("read terminal notification becomes idle", () => {
    expect(deriveTaskStatus({ manualStatus: null, latestSubmittedAt: null,
      notifications: [notification({ subtitle: "Waiting", is_read: true })] })).toBe("idle");
  });
  test("new submission after terminal notification is executing", () => {
    expect(deriveTaskStatus({ manualStatus: null, latestSubmittedAt: "2026-07-10T11:00:00Z",
      notifications: [notification({ subtitle: "Completed", is_read: true })] })).toBe("executing");
  });
  test("manual needs action is respected", () => {
    expect(deriveTaskStatus({ manualStatus: "needs_action", latestSubmittedAt: null, notifications: [] }))
      .toBe("needs_action");
  });
});

test("legacy manual statuses migrate", () => {
  expect(normalizeManualStatus("blocked")).toBe("needs_action");
  expect(normalizeManualStatus("review")).toBe("needs_review");
  expect(normalizeManualStatus("verifying")).toBe("needs_review");
  expect(normalizeManualStatus("done")).toBe("idle");
});
```

- [ ] **Step 3: Run the test and verify failure**

Run: `bun test src/status.test.ts`

Expected: FAIL because `src/status.ts` does not exist.

- [ ] **Step 4: Implement the reducer**

Implement `src/status.ts` using case-insensitive patterns over joined title, subtitle, and body. Only unread notifications affect `needs_action` or `needs_review`; all matching notifications participate in finding the newest terminal timestamp. Return the normalized manual override first, then action, review, submitted-after-terminal execution, then idle. `statusReason` returns the newest matching unread notification text.

- [ ] **Step 5: Add package scripts and verify**

Add:

```json
"test": "bun test",
"check": "bun run build && bun test"
```

Run: `bun test src/status.test.ts && bun run build`

Expected: reducer tests pass and Vite build succeeds.

### Task 2: Typed Rust cmux runtime and snapshot

**Files:**
- Create: `src-tauri/src/cmux_runtime.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces Tauri commands `fetch_cmux_snapshot() -> CmuxSnapshot`, `start_cmux_watcher(app)`, and `focus_cmux_workspace(workspace_ref, workspace_id, window_id) -> Result<(), CmuxError>`.
- Produces serializable `SourceState`, `CmuxSnapshot`, `CmuxError`, and `CmuxErrorCode`.

- [ ] **Step 1: Write failing Rust resolution and error tests**

In `cmux_runtime.rs`, add tests around pure helpers:

```rust
#[test]
fn bundled_application_cli_is_a_candidate() {
    let candidates = cli_candidates(None, None, Some("/Users/test"));
    assert!(candidates.iter().any(|p| p == Path::new("/Applications/cmux.app/Contents/Resources/bin/cmux")));
}

#[test]
fn broken_pipe_maps_to_access_denied() {
    assert_eq!(classify_failure("Failed to write to socket (Broken pipe)"), CmuxErrorCode::AccessDenied);
}

#[test]
fn malformed_json_maps_to_invalid_response() {
    assert_eq!(parse_json("not-json").unwrap_err().code, CmuxErrorCode::InvalidResponse);
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml cmux_runtime`

Expected: FAIL because the module and helpers are not implemented.

- [ ] **Step 3: Implement discovery, execution, and typed errors**

Implement:

```rust
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CmuxErrorCode { CliNotFound, CmuxNotRunning, AccessDenied, Timeout, InvalidResponse, WatcherDisconnected }

#[derive(Debug, Clone, Serialize)]
pub struct CmuxError { pub code: CmuxErrorCode, pub message: String, pub detail: Option<String> }

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SourceState {
    Ready { cli_path: String, socket_path: Option<String> },
    Error { code: CmuxErrorCode, message: String, detail: Option<String> },
}
```

Resolve CLI candidates in the documented order. Resolve the socket from `CMUX_SOCKET_PATH`, then `~/.local/state/cmux/last-socket-path` when it points to a Unix socket. Apply the resolved socket to every child command. Execute with Tokio timeout and preserve stderr.

- [ ] **Step 4: Implement snapshot composition without Python**

Run `ping`, `list-windows --json`, `workspace list --json --id-format both --window <id>` for every window, and `list-notifications --json`. Parse JSON into `serde_json::Value`, add `window_id` to every workspace object, and return one `CmuxSnapshot`. Any command or decode failure returns `source: error` with empty arrays; a successful zero-workspace response returns `source: ready` with empty arrays.

- [ ] **Step 5: Register commands and remove obsolete code**

Replace `fetch_cmux_data`, the embedded `FETCH_SCRIPT`, and generic frontend cmux command usage in `src-tauri/src/lib.rs` with the runtime module commands. Keep `read_home_file`, `write_home_file`, and `home_dir` for local overrides.

- [ ] **Step 6: Verify Rust runtime**

Run: `cargo test --manifest-path src-tauri/Cargo.toml && cargo check --manifest-path src-tauri/Cargo.toml`

Expected: all unit tests pass and the crate checks without the prior unnecessary-`mut` warning.

### Task 3: Structured frontend snapshot and health UI

**Files:**
- Modify: `src/cmux.ts`
- Modify: `src/main.ts`
- Modify: `index.html`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: `CmuxSnapshot` from Task 2 and `deriveTaskStatus` from Task 1.
- Produces: serialized `refresh()` calls, `renderSourceState()`, and active-workspace-only `MergedTask[]`.

- [ ] **Step 1: Replace the error-swallowing adapter**

Make `fetchAll()` call `invoke<CmuxSnapshot>("fetch_cmux_snapshot")` and return the snapshot unchanged. Delete the catch that converts every failure into empty arrays.

- [ ] **Step 2: Refactor task gathering**

In `gatherMergedTasks`, return `{ tasks, source, fetchedAt }`. Iterate current snapshot workspaces only; merge matching config or call `ensureTaskForCmux`; never append configs whose workspace is absent. Compute status through `deriveTaskStatus` and normalize legacy manual overrides.

- [ ] **Step 3: Serialize refreshes and preserve last good data**

Use `refreshInFlight` and `refreshQueued` booleans. A ready snapshot replaces `mergedTasks` and clears stale state. An error snapshot keeps the previous cards, sets `sourceState`, and marks cards stale. An initial error renders only the source diagnosis.

- [ ] **Step 4: Render source health**

Add `<div id="source-status" role="status"></div>` to `index.html`. Render concise Chinese messages for every typed code, including the exact `allowAll` guidance for `ACCESS_DENIED`, plus a retry button. Add `.source-error`, `.source-warning`, and `.stale` styles.

- [ ] **Step 5: Update four-state labels and manual override semantics**

Update `STATUS_META`, context-menu entries, suggestion priority, and all comparisons to the new states. Ensure `needs_action` manual override is respected. When context menu opens, temporarily expand the transparent Tauri window enough to show it; shrink it after selection or dismissal.

- [ ] **Step 6: Verify frontend**

Run: `bun test && bun run build`

Expected: status tests pass and TypeScript/Vite build succeeds.

### Task 4: Event-driven refresh with fallback polling

**Files:**
- Modify: `src-tauri/src/cmux_runtime.rs`
- Modify: `src/main.ts`

**Interfaces:**
- Produces Tauri events `cmux-state-changed` and `cmux-watcher-state`.
- Consumes serialized `refresh()` from Task 3.

- [ ] **Step 1: Implement a singleton reconnecting watcher**

Guard startup with `AtomicBool`. Spawn `cmux events --reconnect --no-ack --no-heartbeat` with the resolved socket. Emit `cmux-state-changed` for each non-empty stdout line. If spawning or reading ends, emit a typed watcher error, clear/re-resolve runtime discovery, and retry after 1, 2, 4, 8, then 15 seconds.

- [ ] **Step 2: Subscribe and debounce in the frontend**

Call `start_cmux_watcher` once and subscribe with `listen`. Reset a 250 ms timer for state-change events and call the serialized refresh when it fires. Keep a 30-second interval as the health fallback. Store and call unlisten functions during page teardown.

- [ ] **Step 3: Verify build and watcher failure behavior**

Run: `cargo test --manifest-path src-tauri/Cargo.toml && bun run build`

Expected: tests and build pass. With cmux inaccessible, the watcher emits an error without crashing or clearing the last snapshot.

### Task 5: Safe cmux jump transaction

**Files:**
- Modify: `src-tauri/src/cmux_runtime.rs`
- Modify: `src/cmux.ts`
- Modify: `src/jump.ts`
- Modify: `src/main.ts`

**Interfaces:**
- Produces backend command `focus_cmux_workspace(workspace_ref: String, workspace_id: String, window_id: String)`.
- Consumes: current card's cmux identifiers.

- [ ] **Step 1: Add failing command-order tests**

Test a pure `jump_commands` helper:

```rust
#[test]
fn jump_focuses_window_before_select_and_marks_read_last() {
    let commands = jump_commands("workspace:2", "uuid-2", "window:1");
    assert_eq!(commands[0].args, vec!["focus-window", "--window", "window:1"]);
    assert_eq!(commands[1].args, vec!["workspace", "select", "workspace:2"]);
    assert_eq!(commands[2].args, vec!["mark-notification-read", "--workspace", "uuid-2"]);
}
```

- [ ] **Step 2: Implement backend action sequencing**

Run `/usr/bin/open -a cmux`, then focus window, select workspace, and finally mark notifications read. Stop on the first error, so mark-read never runs when focus or selection fails. Use the same runtime resolver and typed errors as snapshots.

- [ ] **Step 3: Guarantee frontend window restoration**

In `jumpToCmux`, call `setAlwaysOnTop(false)`, invoke the backend action, and restore `setAlwaysOnTop(true)` in `finally`. Show the typed action error in the toast without replacing source health.

- [ ] **Step 4: Verify**

Run: `cargo test --manifest-path src-tauri/Cargo.toml && bun test && bun run build`

Expected: command-order tests, status tests, and production build pass.

### Task 6: Setup documentation and live acceptance

**Files:**
- Modify: `README.md`
- Create: `scripts/test_cmux_live.py`
- Delete: `test_status.py`
- Delete: `test_e2e.py`

**Interfaces:**
- Live script consumes `CMUX_BUNDLED_CLI_PATH` or the standard application path and optional `CMUX_SOCKET_PATH`.
- Default live mode is read-only; `--jump <workspace-ref>` explicitly enables selection testing.

- [ ] **Step 1: Write the live doctor**

Implement checks for executable discovery, socket discovery, `ping`, window/workspace JSON decoding, and notifications JSON decoding. Report each failure using the same error names as the app. Do not activate cmux, select a workspace, or mark notifications read by default.

- [ ] **Step 2: Replace template README**

Document cmux `allowAll`, CLI/socket discovery, `bun run tauri dev`, `bun test`, `cargo test --manifest-path src-tauri/Cargo.toml`, the live doctor, each source error, and the fact that Focus Bar does not edit cmux settings automatically.

- [ ] **Step 3: Run complete static verification**

Run: `bun run check`

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: every command exits 0.

- [ ] **Step 4: Configure and validate local cmux access**

Back up `~/.config/cmux/cmux.json`, set `automation.socketControlMode` to `allowAll` using cmux-supported configuration, and reload or restart cmux with explicit user approval if required. Run `python3 scripts/test_cmux_live.py`; expect healthy source and decoded workspaces.

- [ ] **Step 5: Run manual Tauri acceptance**

Start `bun run tauri dev`. Confirm current workspaces render without historical duplicates, a cmux state event refreshes the bar, source errors remain visible, cmux restart recovery works, and clicking a chosen card focuses its correct window/workspace before clearing unread notifications.

- [ ] **Step 6: Record final verification evidence**

Capture exact exit results for frontend tests/build, Rust tests/check, live doctor, and the five manual acceptance observations. Do not claim completion unless all required checks pass.
