# Task-State Time Tracking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace foreground-app dwell statistics with per-task execution and closed-interruption timing derived from the same red/green states shown by Focus Bar.

**Architecture:** The TypeScript main window submits each successful cmux/Codex task-status snapshot to a Rust command. Rust persists a compact per-task current state plus completed execution/interruption intervals in SQLite, then aggregates per-task totals, summed task execution, and the union of concurrent execution intervals. The stats window renders only this new summary; the old foreground activity database remains untouched and its sampling loop is no longer started.

**Tech Stack:** Tauri 2, Rust, serde/serde_json, system sqlite3 CLI, TypeScript, Bun tests, Vite.

## Global Constraints

- `executing` is execution time; `needs_action` and `needs_review` start one pending interruption; `idle` neither executes nor clears that pending interruption.
- A pending interruption is persisted only when the same task later becomes `executing`; an unclosed tail is discarded.
- Sum concurrent task execution for “任务执行量” and merge it for “实际运行覆盖”.
- A sampling gap over 15 seconds breaks continuity and must not be backfilled.
- Preserve the old `activity_segments` table and data, but stop the old foreground activity loop.
- Work in the current workspace; do not create a worktree or dispatch subagents.

---

### Task 1: Pure task timing reducer and aggregation

**Files:**
- Create: `src-tauri/src/task_timing.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces `TaskStatus`, `TaskStatusSample`, `TaskTimingState`, `CompletedInterval`, `TaskTimingSummary`, `update_task_state(...)`, and `aggregate_task_timing(...)` for persistence/runtime consumers.
- `update_task_state(previous, sample, observed_at, max_gap_ms)` returns the new state plus zero or more completed execution/interruption intervals.

- [ ] Write Rust tests for execution close, red→idle→green interruption close, unclosed red discard, gap reset, concurrent sum versus union, and range clipping.
- [ ] Run `cargo test task_timing --manifest-path src-tauri/Cargo.toml` and verify the new tests fail because the module/API is absent.
- [ ] Implement the minimal pure reducer and interval aggregator.
- [ ] Re-run the focused Rust tests and verify they pass.

### Task 2: SQLite persistence and Tauri commands

**Files:**
- Create: `src-tauri/src/task_timing_store.rs`
- Create: `src-tauri/src/task_timing_runtime.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes reducer types from Task 1.
- Produces Tauri commands `record_task_status_snapshot(tasks: Vec<TaskStatusSample>, observed_at: u64)` and `fetch_task_timing_summary(range_start: u64, range_end: u64)`.
- Persists `task_timing_state`, `task_execution_intervals`, and `task_interruption_intervals` in the existing activity SQLite file without modifying `activity_segments`.

- [ ] Write store tests using a temporary SQLite file: same-status heartbeat updates `last_seen_at`, status changes persist execution, red→green persists interruption, and a gap does not bridge intervals.
- [ ] Run the focused Rust tests and verify failure before implementation.
- [ ] Implement schema initialization, state loading/upsert, completed interval insertion, range loading, and runtime commands.
- [ ] Remove the old `activity_runtime::start(...)` call while leaving old tables and code intact.
- [ ] Re-run focused Rust tests and `cargo test --manifest-path src-tauri/Cargo.toml`.

### Task 3: Submit reliable task-state snapshots

**Files:**
- Modify: `src/main.ts`
- Create: `src/task-timing.ts`
- Create: `src/task-timing.test.ts`

**Interfaces:**
- Produces `taskStatusSamples(cmuxTasks, codexTasks)` returning `{ task_id, task_title, source, status }[]`.
- `refreshOnce()` submits only newly successful source snapshots with one shared `observedAt` timestamp.

- [ ] Write Bun tests showing cmux/Codex statuses map identically and failed-source stale arrays are excluded by passing only fresh arrays.
- [ ] Run `bun test src/task-timing.test.ts` and verify failure before implementation.
- [ ] Implement the pure sample builder and invoke `record_task_status_snapshot` after each successful refresh.
- [ ] Re-run focused Bun tests.

### Task 4: Replace the statistics presentation

**Files:**
- Create: `src/task-timing-summary.ts`
- Create: `src/task-timing-summary.test.ts`
- Modify: `src/stats.ts`
- Modify: `src/stats.css`
- Modify: `stats.html`

**Interfaces:**
- Consumes Rust summary `{ task_execution_ms, actual_execution_ms, tasks }`.
- Produces pure `renderTaskTimingSummary(summary, period)` HTML for the stats window.

- [ ] Write Bun tests for the two overview totals, per-task execution/interruption/round counts, empty state, and today/week labels.
- [ ] Run the focused Bun tests and verify failure before implementation.
- [ ] Implement summary types, HTML renderer, Tauri fetch integration, and the two-card/task-metric layout.
- [ ] Update copy to describe task-state timing and remove the 90-second foreground-idle explanation.
- [ ] Re-run focused Bun tests.

### Task 5: Full verification and live app validation

**Files:**
- Modify only files required by failures found during verification.

- [ ] Run `bun run check` and confirm zero failures.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml` and confirm zero failures.
- [ ] Run `bun run tauri build --bundles app` and confirm the signed app bundle is produced.
- [ ] Install `/Applications/focus-bar.app`, restart it, open statistics, and visually verify the two totals and per-task rows.
- [ ] Verify `codesign --verify --deep --strict /Applications/focus-bar.app` succeeds.

