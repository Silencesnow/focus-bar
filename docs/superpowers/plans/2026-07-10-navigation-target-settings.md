# Navigation Target Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a dedicated settings window that binds each active cmux workspace to an exact Google Chrome URL and an official VS Code workspace, with safe persistence, explicit test actions, and reliable overlay navigation.

**Architecture:** A new pure TypeScript configuration module handles form normalization and validation. Typed Rust commands own atomic `~/.focus.json` merges and process-safe Chrome/VS Code automation, while a second Vite/Tauri page provides the settings UI and application events keep the overlay synchronized.

**Tech Stack:** Tauri 2, Rust 2021, Tokio, Serde/serde_json, TypeScript 5.6, Bun test runner, Vite 6, macOS AppleScript/System Events, Google Chrome, official Visual Studio Code.

## Global Constraints

- Support Google Chrome and official Visual Studio Code only.
- Chrome matching is exact URL equality; a missing URL opens in a new tab.
- VS Code uses an absolute workspace directory and optional workspace name, relative file, and positive line.
- User values are process arguments and are never interpolated into AppleScript source.
- Navigation persistence preserves cmux ID, manual status, note, and unrelated task fields.
- Saving emits `focus-config-changed`; the overlay refreshes immediately.
- Explicit `🌐` and `📝` icons navigate; existing card-body smart navigation remains unchanged.

## File Structure

- Create `src/navigation-config.ts`: form normalization, validation, dirty comparison, and error-message mapping.
- Create `src/navigation-config.test.ts`: Bun tests for form/config behavior.
- Create `src-tauri/src/navigation.rs`: typed navigation errors, Chrome AppleScript, VS Code target resolution, and commands.
- Create `src-tauri/src/task_config.rs`: navigation DTOs, validation, atomic config merge, and save command.
- Modify `src-tauri/src/lib.rs`: register navigation/config commands and remove Chrome/VS Code use of generic shell execution.
- Modify `src/types.ts`: add `workspace_name` and typed navigation errors/payloads.
- Create `settings.html`, `src/settings.ts`, and `src/settings.css`: settings page.
- Modify `vite.config.ts`: build both HTML entries.
- Modify `src-tauri/tauri.conf.json`: declare hidden `settings` window.
- Modify `src-tauri/capabilities/default.json`: grant both windows required core permissions.
- Modify `index.html`, `src/main.ts`, and `src/styles.css`: gear button, task configuration action, config-change listener.
- Modify `src/jump.ts`: call typed Chrome/VS Code commands.
- Modify `README.md`: configuration and permission guidance.

---

### Task 1: Navigation configuration model and validation

**Files:**
- Create: `src/navigation-config.ts`
- Create: `src/navigation-config.test.ts`
- Modify: `src/types.ts`
- Modify: `tsconfig.json`

**Interfaces:**
- Consumes `TaskConfig`, form strings, and `NavigationErrorCode`.
- Produces `NavigationForm`, `formFromTask(task)`, `normalizeNavigationForm(form)`, `validateNavigationForm(form)`, `formsEqual(a, b)`, and `navigationErrorMessage(error)`.

- [ ] **Step 1: Write failing Bun tests**

Create tests proving:

```ts
test("normalizes empty target groups away", () => {
  expect(normalizeNavigationForm(emptyForm())).toEqual({ name: "Task", chrome: null, vscode: null });
});

test("accepts an exact https Chrome URL", () => {
  const form = emptyForm({ chromeUrl: "https://example.com/path?q=1#result" });
  expect(validateNavigationForm(form)).toEqual([]);
});

test("rejects a relative VS Code workspace", () => {
  const form = emptyForm({ vscodeWorkspace: "packages/app" });
  expect(validateNavigationForm(form)).toContain("VS Code workspace 必须是绝对目录");
});

test("rejects a file that escapes its workspace", () => {
  const form = emptyForm({ vscodeWorkspace: "/tmp/app", vscodeFile: "../secret" });
  expect(validateNavigationForm(form)).toContain("文件路径不能离开 workspace");
});

test("prefills existing navigation fields", () => {
  expect(formFromTask(configWithTargets()).chromeUrl).toBe("https://example.com");
  expect(formFromTask(configWithTargets()).vscodeWorkspaceName).toBe("app");
});
```

- [ ] **Step 2: Run RED verification**

Run: `bun test src/navigation-config.test.ts`

Expected: FAIL because `src/navigation-config.ts` does not exist.

- [ ] **Step 3: Add exact shared types**

Add `workspace_name?: string` to `VscodeTarget`. Add:

```ts
export type NavigationErrorCode =
  | "INVALID_TARGET"
  | "CHROME_NOT_INSTALLED"
  | "VSCODE_NOT_INSTALLED"
  | "AUTOMATION_PERMISSION_REQUIRED"
  | "ACCESSIBILITY_PERMISSION_REQUIRED"
  | "TARGET_COMMAND_FAILED"
  | "TARGET_TIMEOUT";

export interface NavigationError {
  code: NavigationErrorCode;
  message: string;
  detail?: string | null;
}
```

- [ ] **Step 4: Implement minimal pure functions**

`normalizeNavigationForm` trims values, parses a positive integer line, derives `workspace_name` from the workspace basename when blank, returns absent target groups as `null`, and preserves an exact Chrome URL string. `validateNavigationForm` accepts only `http:`/`https:`, absolute workspaces, relative non-escaping files, and positive lines.

- [ ] **Step 5: Verify GREEN and commit**

Run: `bun test src/navigation-config.test.ts && bun run build`

Expected: all new tests pass and production TypeScript builds.

Commit: `feat: add navigation target validation`

### Task 2: Typed Rust Chrome and VS Code navigation

**Files:**
- Create: `src-tauri/src/navigation.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/jump.ts`

**Interfaces:**
- Produces `focus_chrome_url(url: String) -> Result<(), NavigationError>`.
- Produces `focus_vscode_target(workspace: String, workspace_name: String, file: Option<String>, line: Option<u32>) -> Result<(), NavigationError>`.
- Consumes existing `TaskConfig.chrome` and `TaskConfig.vscode` through `jump.ts`.

- [ ] **Step 1: Write failing Rust unit tests**

Tests must assert:

```rust
#[test]
fn chrome_url_is_an_argument_not_script_source() {
    let hostile = "https://example.com/\" & do shell script \"bad\"";
    let spec = chrome_command(hostile).unwrap();
    assert!(!spec.script.contains(hostile));
    assert_eq!(spec.args.last().unwrap(), hostile);
}

#[test]
fn vscode_file_target_includes_line() {
    let target = vscode_goto_target(Path::new("/tmp/app"), Some("src/main.ts"), Some(42)).unwrap();
    assert_eq!(target.unwrap(), "/tmp/app/src/main.ts:42");
}

#[test]
fn vscode_file_cannot_escape_workspace() {
    assert_eq!(vscode_goto_target(Path::new("/tmp/app"), Some("../secret"), None).unwrap_err().code,
               NavigationErrorCode::InvalidTarget);
}
```

- [ ] **Step 2: Run RED verification**

Run: `cargo test --manifest-path src-tauri/Cargo.toml navigation`

Expected: FAIL because the module and functions do not exist.

- [ ] **Step 3: Implement static-script command construction**

Create serializable `NavigationErrorCode` and `NavigationError`. Build `/usr/bin/osascript` commands as `-e <static-script> -- <user-args>`. Resolve official `code` from `PATH`, `/usr/local/bin/code`, `/opt/homebrew/bin/code`, and `/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code`.

- [ ] **Step 4: Implement Chrome behavior**

Run a static `on run argv` script that compares every tab URL using exact equality, selects the match or calls `open location`, and activates Chrome. Map `-1743` or “not authorized” to `AUTOMATION_PERMISSION_REQUIRED`; map missing Chrome to `CHROME_NOT_INSTALLED`; cap execution at 8 seconds.

- [ ] **Step 5: Implement VS Code behavior**

Validate workspace/file paths, then use a static System Events script with workspace name in `argv` to raise an existing `Code` window. If not found, run `code <workspace>`. If a file exists, run `code --goto <absolute-file[:line]>`. Map accessibility denial to `ACCESSIBILITY_PERMISSION_REQUIRED` and missing CLI/app to `VSCODE_NOT_INSTALLED`.

- [ ] **Step 6: Route frontend jumps through typed commands**

Replace AppleScript construction in `src/jump.ts` with `invoke("focus_chrome_url", { url })` and `invoke("focus_vscode_target", { workspace, workspaceName, file, line })`. Preserve current `jumpSmart` ordering.

- [ ] **Step 7: Verify and commit**

Run: `cargo test --manifest-path src-tauri/Cargo.toml && cargo check --manifest-path src-tauri/Cargo.toml && bun run build`

Expected: Rust tests/check and frontend build pass.

Commit: `feat: add typed Chrome and VS Code navigation`

### Task 3: Atomic task navigation persistence

**Files:**
- Create: `src-tauri/src/task_config.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types.ts`

**Interfaces:**
- Produces `save_task_navigation(app, task_id, name, chrome, vscode) -> Result<serde_json::Value, NavigationError>`.
- Emits `focus-config-changed` with task ID after a successful atomic write.

- [ ] **Step 1: Write failing Rust merge tests**

Use a temporary directory fixture and assert that updating navigation changes only `name`, `chrome`, and `vscode`, while preserving `cmux_workspace_id`, `manual_status`, `note`, and unknown keys. Add tests for missing task ID, invalid URL, relative workspace, escaping file, and non-positive line.

- [ ] **Step 2: Run RED verification**

Run: `cargo test --manifest-path src-tauri/Cargo.toml task_config`

Expected: FAIL because the module is absent.

- [ ] **Step 3: Implement merge and atomic write**

Read JSON into `serde_json::Value`, locate `tasks[*].id`, validate typed navigation payloads, update only the allowed keys, write `<focus-path>.tmp`, call `sync_all`, and rename over the original. Remove empty `chrome`/`vscode` properties rather than writing empty objects.

- [ ] **Step 4: Register command and event**

Register `save_task_navigation`; after rename succeeds emit `focus-config-changed` with `{ task_id }`. Return the updated task object for settings-form state.

- [ ] **Step 5: Verify and commit**

Run: `cargo test --manifest-path src-tauri/Cargo.toml task_config && cargo check --manifest-path src-tauri/Cargo.toml`

Expected: merge/error tests pass and crate checks.

Commit: `feat: persist navigation targets atomically`

### Task 4: Dedicated settings window

**Files:**
- Create: `settings.html`
- Create: `src/settings.ts`
- Create: `src/settings.css`
- Modify: `vite.config.ts`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/capabilities/default.json`

**Interfaces:**
- Consumes `fetchAll`, `readFocusData`, Task 1 pure functions, Task 2 test commands, and Task 3 save command.
- Produces a two-pane settings UI for active cmux workspaces.

- [ ] **Step 1: Add the second build/window entry**

Configure Vite Rollup input for absolute `index.html` and `settings.html`. Add a hidden `settings` Tauri window with label/title `settings`/`Focus Bar Settings`, URL `settings.html`, dimensions 560×480, decorations, center, and minimum size. Include `settings` in capability window labels and allow show/hide/focus/close handling.

- [ ] **Step 2: Implement settings HTML and styles**

Create semantic workspace list, labeled form fields, save/test/clear buttons, and `role=status` feedback. Keep the form usable at the declared minimum size, with keyboard focus states and no dependency on external UI libraries.

- [ ] **Step 3: Implement settings data loading**

Fetch a ready cmux snapshot and local configs, render active workspaces only, prefill selection using `formFromTask`, and listen for `open-settings-for-task`. If source is unavailable, show the same typed cmux source guidance as the overlay.

- [ ] **Step 4: Implement dirty-state and actions**

Track saved/current forms with `formsEqual`. Confirm before discarding edits on task switch. Save through `save_task_navigation`; test unsaved values through typed navigation commands; clear each target group and save. Render validation and typed permission errors inline.

- [ ] **Step 5: Verify and commit**

Run: `bun test && bun run build && cargo check --manifest-path src-tauri/Cargo.toml`

Expected: all tests and both Vite entries build; Tauri config validates during cargo check.

Commit: `feat: add navigation settings window`

### Task 5: Overlay integration, docs, and live acceptance

**Files:**
- Modify: `index.html`
- Modify: `src/main.ts`
- Modify: `src/styles.css`
- Modify: `README.md`

**Interfaces:**
- Opens/focuses `settings` window and optionally emits selected task ID.
- Listens for `focus-config-changed` and refreshes overlay icons immediately.

- [ ] **Step 1: Add settings entry points**

Add a non-dragging gear button to the overlay. Add `配置跳转目标` to the task context menu. Both show/focus the settings window; the context action emits the selected task ID.

- [ ] **Step 2: Add immediate config refresh**

Listen for `focus-config-changed`, call the existing serialized `refresh`, and remove the listener during teardown. Verify saved Chrome/VS Code targets change icons without waiting for the 30-second interval.

- [ ] **Step 3: Update documentation**

Document opening settings, every field, exact Chrome matching/open-if-missing behavior, VS Code matching, macOS Automation/Accessibility permissions, test buttons, and typed errors.

- [ ] **Step 4: Run full automated verification**

Run: `bun run check`

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: every command exits 0 with no test failures.

- [ ] **Step 5: Run live settings and navigation acceptance**

Start `bun run tauri dev`. Configure one active task. Verify save updates overlay icons immediately; exact existing Chrome URL activates its tab; closing the tab makes test open it; open VS Code workspace is raised; closed workspace is opened; optional file/line navigates; clear actions remove icons. Capture any macOS permission prompt/error and confirm the UI guidance matches it.

- [ ] **Step 6: Commit the integrated feature**

Commit: `feat: integrate navigation target settings`
