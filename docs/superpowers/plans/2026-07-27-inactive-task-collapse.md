# Inactive Task Collapse Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collapse idle tasks without activity today into a default-closed “较早任务（N）” section while preserving every existing task action.

**Architecture:** Add a pure task grouping module that owns the local-midnight classification and expanded visibility rules. Keep the complete `mergedTasks` collection as the source of truth; `main.ts` only groups it for rendering and resolves clicks by stable config ID instead of array position.

**Tech Stack:** TypeScript, Bun tests, Tauri webview HTML/CSS.

## Global Constraints

- Red pending tasks and green executing tasks always remain in the main list.
- Only gray idle tasks with missing, invalid, or pre-midnight `activityAt` enter the older section.
- The older section is collapsed by default and is not persisted across app restarts.
- Existing card click, context menu, cmux, Codex, VS Code, and Chrome actions remain unchanged.
- Work in the current workspace; do not create a git worktree.

---

### Task 1: Pure inactive-task grouping

**Files:**
- Create: `src/task-groups.ts`
- Create: `src/task-groups.test.ts`

**Interfaces:**
- Consumes: `MergedTask.effectiveStatus` and `MergedTask.activityAt`.
- Produces: `groupTasksByToday(tasks: MergedTask[], now?: number): { current: MergedTask[]; inactive: MergedTask[] }`.
- Produces: `taskListModel(tasks: MergedTask[], inactiveExpanded: boolean, now?: number)` with grouped tasks, visible tasks, and toggle metadata.
- Produces: `findTaskByConfigId(tasks: MergedTask[], taskId: string): MergedTask | null`.

- [x] **Step 1: Write failing grouping tests**

Cover an idle task active after local midnight, idle tasks from yesterday or with invalid/missing time, pending/executing tasks with old time, collapsed versus expanded visibility, and lookup by stable config ID.

- [x] **Step 2: Run the focused test and verify RED**

Run: `bun test src/task-groups.test.ts`

Expected: failure because `task-groups.ts` does not exist.

- [x] **Step 3: Implement the grouping module**

Compute local midnight with:

```ts
const start = new Date(now);
start.setHours(0, 0, 0, 0);
```

Classify only `idle` tasks with an invalid timestamp or a timestamp before `start.getTime()` as inactive. Preserve input order in both groups.

- [x] **Step 4: Run the focused test and verify GREEN**

Run: `bun test src/task-groups.test.ts`

Expected: all grouping tests pass.

### Task 2: Collapsible rendering with stable task identity

**Files:**
- Modify: `src/main.ts`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: `taskListModel` and `findTaskByConfigId` from Task 1.
- Preserves: `jumpSmart`, `handleExplicitJump`, context menu, and suggestion behavior.

- [x] **Step 1: Implement collapsed rendering**

Add process-local `inactiveTasksExpanded = false`. In `render()`:

```ts
const model = taskListModel(mergedTasks, inactiveTasksExpanded);
const currentHtml = model.current.map(renderCard).join("");
const inactiveHtml = inactiveTasksExpanded
  ? model.inactive.map(renderCard).join("")
  : "";
```

Render “较早任务（N）” only when `groups.inactive.length > 0`; toggling it updates state and rerenders. If no current tasks exist but inactive tasks do, render the toggle instead of the empty state.

Change cards to `data-task-id="<config.id>"`. Resolve click and context-menu targets with `findTaskByConfigId(mergedTasks, taskId)` so grouping never changes navigation identity.

- [x] **Step 2: Add compact section styling**

Style the toggle as a subdued full-width row with a visible hover response, count, and rotating disclosure indicator. Keep expanded older cards in the existing vertical flow and scroll container.

- [x] **Step 3: Run focused tests and verify GREEN**

Run: `bun test src/task-groups.test.ts`

Expected: all focused tests pass.

### Task 3: Full verification and app installation

**Files:**
- Verify all changed source and tests.
- Build: `src-tauri/target/release/bundle/macos/focus-bar.app`

**Interfaces:**
- Consumes the completed grouping and rendering behavior.
- Produces the installed `/Applications/focus-bar.app`.

- [x] **Step 1: Run the full project check**

Run: `bun run check`

Expected: TypeScript build and all Bun tests pass with zero failures.

- [x] **Step 2: Build the macOS app**

Run: `bun run tauri build --bundles app`

Expected: release binary and signed local `.app` bundle are produced successfully.

- [x] **Step 3: Install and restart**

Copy the built bundle to `/Applications/focus-bar.app`, stop the existing Focus Bar process, and launch the installed app.

- [x] **Step 4: Verify the installed process**

Confirm `/Applications/focus-bar.app/Contents/MacOS/focus-bar` is running and report the final test count.
