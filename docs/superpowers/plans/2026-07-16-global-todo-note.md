# Global Todo Note Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a persistent five-line global todo note below the Focus Bar task list.

**Architecture:** Store `global_note` at the top level of `~/.focus.json`. A focused `global-note.ts` module owns debounce timing and latest-data-preserving writes; `main.ts` only binds the textarea, status text, and window sizing.

**Tech Stack:** TypeScript, Tauri invoke storage, Bun tests, HTML/CSS.

## Global Constraints

- The note is global rather than task-specific.
- The textarea displays 5 rows by default and supports line breaks.
- Save 300 ms after the latest input and preserve unrelated config changes.
- Keep old `.focus.json` files compatible when `global_note` is absent.
- Do not let note interaction drag the native window.

---

### Task 1: Persistent note model and autosave controller

**Files:**
- Create: `src/global-note.ts`
- Create: `src/global-note.test.ts`
- Modify: `src/types.ts`
- Modify: `src/store.ts`

**Interfaces:**
- Produces: `setGlobalNote(data: FocusData, note: string): FocusData`
- Produces: `saveGlobalNote(note: string): Promise<void>`
- Produces: `createGlobalNoteAutosave(save, delay): { schedule(note, onState), flush(), cancel() }`

- [ ] Write failing tests proving `setGlobalNote` preserves tasks and autosave collapses rapid edits to the latest value.
- [ ] Run `bun test src/global-note.test.ts`; expect missing-export failures.
- [ ] Add optional `global_note?: string` to `FocusData`, immutable note updates in `store.ts`, and a 300 ms autosave controller in `global-note.ts`.
- [ ] Run `bun test src/global-note.test.ts`; expect all tests to pass.

### Task 2: Five-line note UI and installed-app verification

**Files:**
- Modify: `index.html`
- Modify: `src/main.ts`
- Modify: `src/styles.css`
- Modify: `src/window-config.test.ts`

**Interfaces:**
- Consumes: `readFocusData()` and `createGlobalNoteAutosave()`.
- Produces: `#global-note` textarea and `#global-note-status` feedback element.

- [ ] Write a failing DOM/CSS regression test requiring `rows="5"`, note placement before `#resize-handle`, drag exclusion, and note height in `fitWindowToTasks()`.
- [ ] Run `bun test src/window-config.test.ts`; expect the new assertions to fail.
- [ ] Add the note markup and compact styling, load its value once at startup, schedule saves on input, flush on blur/unload, and include its height in window sizing.
- [ ] Run `npm test -- --run` and `cargo test --manifest-path src-tauri/Cargo.toml`; expect zero failures apart from documented ignored live tests.
- [ ] Build with `npm run tauri build -- --bundles app`, install to `/Applications/focus-bar.app`, restart, and verify typing, save feedback, persisted `global_note`, and five visible rows.
