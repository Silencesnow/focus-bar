# Navigation Target Settings Design

## Goal

Add a dedicated configuration window that lets the user bind each active cmux workspace to one Google Chrome URL and one Visual Studio Code workspace. The Focus Bar must then expose explicit Chrome and VS Code buttons that reliably focus the configured target, with test actions and actionable macOS permission errors.

## Scope

Included:

- Google Chrome only;
- official Visual Studio Code (`Code`) only;
- one Chrome target and one VS Code target per cmux workspace;
- a separate settings window;
- exact Chrome URL matching with open-if-missing behavior;
- VS Code matching by configured workspace name and absolute directory;
- optional VS Code file and line target;
- save, test, and clear actions; and
- immediate refresh of the Focus Bar after saving.

Excluded:

- Arc, Safari, Edge, Cursor, and VS Code forks;
- Chrome URL prefix, origin, regex, or title matching;
- discovering the current Chrome tab or VS Code window automatically;
- multiple browser or editor targets per task; and
- changing the existing attention-status model or cmux behavior.

## Confirmed Decisions

- Chrome first attempts an exact URL match across every Chrome tab. If no exact match exists, it opens the configured URL in a new tab.
- The VS Code workspace directory is the stable target identifier. Workspace name is stored for display and existing-window matching.
- Configuration remains in `~/.focus.json` and is merged into the existing task record.
- Explicit `🌐` and `📝` icons trigger browser and editor navigation. Card-body smart navigation remains unchanged.
- User-provided strings are passed as process arguments; they are never interpolated into AppleScript source.

## Considered Approaches

### A. Dedicated Tauri settings window — selected

A normal decorated window provides enough space for a workspace list, form validation, test results, and future workflow settings. It does not interfere with the transparent 80-pixel overlay or its temporary context-menu expansion.

### B. Expand the overlay into a settings form

This avoids another entry page but creates fragile interactions among transparency, screen position, dragging, context menus, and focus. It also scales poorly as settings grow.

### C. Continue editing `~/.focus.json` manually

This adds almost no code but prevents safe validation and test actions and is not appropriate for daily use.

## Window Architecture

Add a second Tauri window with label `settings`, initially hidden, using `settings.html`. It is decorated, centered, approximately 560×480 pixels, and resizable down to a usable minimum. The existing `main` window remains unchanged.

Vite builds two HTML entries:

- `index.html` with `src/main.ts` for the overlay;
- `settings.html` with `src/settings.ts` and `src/settings.css` for configuration.

The Tauri capability includes both `main` and `settings`. A gear button in the overlay and a `配置跳转目标` task context action show and focus the settings window. Opening from a task passes the task ID in an application event so the corresponding workspace is selected.

## Settings User Interface

The left pane lists workspaces from the latest ready cmux snapshot. Each row displays workspace title and current directory. Historical task records that are not currently open remain hidden, consistent with the overlay.

The right pane edits the selected task:

- display name;
- Chrome URL;
- VS Code workspace name;
- VS Code workspace absolute directory;
- optional file path relative to the workspace; and
- optional positive line number.

Actions:

- `保存`: validate and merge the navigation fields into the task record;
- `测试 Chrome`: validate and execute the unsaved Chrome value;
- `测试 VS Code`: validate and execute the unsaved VS Code values;
- `清除 Chrome` and `清除 VS Code`: clear the corresponding form group and save; and
- inline success or typed error feedback.

Switching tasks with unsaved edits asks for confirmation before discarding the edits. Closing the settings window hides it rather than exiting the app.

## Data Model

The existing structures become:

```ts
export interface VscodeTarget {
  workspace: string;
  workspace_name?: string;
  file?: string;
  line?: number;
}

export interface ChromeTarget {
  url: string;
}
```

Empty or whitespace-only target groups are stored as absent properties, not empty objects. Existing `vscode.workspace`, `vscode.file`, `vscode.line`, and `chrome.url` data remains compatible.

## Configuration Persistence

Introduce a typed Rust command `save_task_navigation` instead of writing the whole file from the settings frontend. The command:

1. reads the latest `~/.focus.json`;
2. finds the task by stable task ID;
3. validates and updates only `name`, `chrome`, and `vscode`;
4. preserves cmux binding, manual status, note, and unrelated fields;
5. writes through a temporary file and atomic rename; and
6. emits `focus-config-changed` after success.

The overlay listens for `focus-config-changed` and refreshes immediately. This avoids lost updates between the overlay and settings windows.

Validation rules:

- Chrome URL must use `http` or `https` and include a host;
- VS Code workspace must be an absolute directory path;
- workspace name defaults to the directory basename when blank;
- file path must be relative and may not escape the workspace with `..`;
- line must be absent or a positive integer; and
- at least one target group may be configured, but neither is required.

## Chrome Navigation

Add the typed Rust command `focus_chrome_url(url)`.

The backend validates the URL, then launches a static AppleScript with the URL provided in `argv`. The script:

1. iterates every Chrome window and tab;
2. compares each tab URL to the target with exact string equality;
3. activates the matching tab and moves its window to the front; or
4. calls `open location` with the target URL when no tab matches; and
5. activates Google Chrome.

The configured URL is never embedded in AppleScript source. A URL with quotes, backslashes, Unicode, query parameters, or fragments remains data rather than executable script.

## VS Code Navigation

Add the typed Rust command `focus_vscode_target(target)`.

The backend resolves `code` from `PATH` and standard official VS Code locations. It runs a static System Events AppleScript with workspace name supplied in `argv` to raise an existing `Code` window whose title contains that name.

- If a matching window exists and no file is configured, navigation succeeds after raising it.
- If no matching window exists, run `code <absolute-workspace-directory>` to open or focus the workspace.
- If a file is configured, run `code --goto <workspace/file[:line]>` after raising or opening the workspace.

Paths are passed as separate process arguments and never shell-concatenated. The backend rejects relative workspace paths and file paths that escape the workspace.

## Errors and Permissions

Typed navigation errors include:

- `INVALID_TARGET`: form values do not satisfy validation;
- `CHROME_NOT_INSTALLED`: Google Chrome is unavailable;
- `VSCODE_NOT_INSTALLED`: official VS Code or its CLI is unavailable;
- `AUTOMATION_PERMISSION_REQUIRED`: macOS denied control of Google Chrome;
- `ACCESSIBILITY_PERMISSION_REQUIRED`: macOS denied System Events window control;
- `TARGET_COMMAND_FAILED`: the target application returned another failure; and
- `TARGET_TIMEOUT`: navigation did not finish within its deadline.

The settings window and overlay convert these codes into Chinese guidance. Permission failures identify the relevant System Settings pane and keep the configuration intact so the user can retry after granting permission.

The existing generic `shell_output` command is no longer used by Chrome or VS Code navigation. Navigation uses only typed backend commands with validated inputs.

## Testing

### TypeScript tests

- form-to-config normalization;
- existing config prefill;
- dirty-form detection;
- URL and path validation messages; and
- handling of `focus-config-changed` refresh events.

### Rust tests

- exact Chrome AppleScript remains static for hostile-looking URLs;
- URL appears only in the process argument list;
- VS Code CLI resolution candidates;
- workspace/file/line target construction;
- relative and escaping paths are rejected;
- atomic config merge preserves status, note, and cmux identifiers; and
- typed mapping of AppleScript permission errors.

### Live acceptance

1. Configure and save a Chrome URL, then verify the existing exact tab is activated.
2. Close that tab and verify the same test opens a new tab.
3. Configure a currently open VS Code workspace and verify its window is raised.
4. Close the workspace and verify the directory is opened again.
5. Configure a file and line and verify VS Code navigates to it.
6. Save from settings and verify overlay icons update immediately.
7. Deny or remove the relevant permission and verify actionable error feedback.

## Success Criteria

The feature is accepted when the user can configure either target without editing JSON, save it without losing other task data, test it from the settings window, and use overlay icons to reliably focus an exact Chrome URL or configured official VS Code workspace with clear permission and validation errors.
