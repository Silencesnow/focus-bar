# Tab and Tool Icons Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every task Tab an independent emoji/letter icon and replace the cmux, VS Code, and Chrome action emoji with bundled application icons.

**Architecture:** A pure `tab-icon.ts` module normalizes configured icons and derives title fallbacks. `tab_icon` flows through the existing settings form and Rust atomic configuration merge; Vite imports three bundled PNG assets for action buttons.

**Tech Stack:** TypeScript, Bun tests, Rust/Serde JSON merge, Tauri/Vite, CSS, macOS ICNS assets.

## Global Constraints

- Tab icons are independent of task source, status, and navigation tools.
- Accept one emoji or at most two letters without splitting a composed emoji.
- Status remains visible through card border and background colors.
- Preserve all existing navigation, active-state, accessibility, and multi-link behavior.
- Continue working with old tasks that do not contain `tab_icon`.

---

### Task 1: Tab icon model, settings, and rendering

**Files:**
- Create: `src/tab-icon.ts`
- Create: `src/tab-icon.test.ts`
- Modify: `src/types.ts`
- Modify: `src/navigation-config.ts`
- Modify: `src/navigation-config.test.ts`
- Modify: `settings.html`
- Modify: `src/settings.ts`
- Modify: `src/settings.css`
- Modify: `src-tauri/src/task_config.rs`
- Modify: `src/main.ts`
- Modify: `src/styles.css`

**Interfaces:**
- Produces: `normalizeTabIcon(value: string): string`
- Produces: `tabIconForTask(configured: string | undefined, title: string): string`
- Adds: `TaskConfig.tab_icon?: string`, `NavigationForm.tabIcon: string`, and `save_task_navigation(icon: String)`.

- [ ] Write failing tests for a single emoji, a composed emoji, two letters, empty fallback, English initials, and a Chinese initial.
- [ ] Run `bun test src/tab-icon.test.ts`; expect missing-module failure.
- [ ] Implement grapheme-safe normalization and title-derived fallback in `src/tab-icon.ts`.
- [ ] Extend navigation form tests to require `tabIcon` prefill, normalization, and dirty-state comparison; run them red, then update TypeScript form types and settings controls.
- [ ] Extend the Rust merge test to require storing and clearing `tab_icon`; run it red, then add the icon argument to `merge_task_navigation` and `save_task_navigation`.
- [ ] Render `.tab-icon` instead of the status emoji and style it as a compact fixed badge while leaving `--status-color` on the card.
- [ ] Run `bun test src/tab-icon.test.ts src/navigation-config.test.ts` and `cargo test --manifest-path src-tauri/Cargo.toml task_config`; expect all targeted tests to pass.

### Task 2: Native tool assets and installed application

**Files:**
- Create: `src/assets/tool-icons/cmux.png`
- Create: `src/assets/tool-icons/vscode.png`
- Create: `src/assets/tool-icons/chrome.png`
- Modify: `src/main.ts`
- Modify: `src/styles.css`
- Modify: `src/activity-line.test.ts`

**Interfaces:**
- Consumes: Vite asset URL imports for each PNG.
- Produces: `.tool-app-icon` images inside existing action buttons.

- [ ] Add failing markup assertions requiring imported cmux, VS Code, and Chrome assets and forbidding `📟`, `📝`, and `🌐` in card rendering.
- [ ] Run `bun test src/activity-line.test.ts`; expect the new assertions to fail.
- [ ] Convert the installed applications' ICNS resources to 64×64 PNG assets and import them in `main.ts`.
- [ ] Replace only the visual contents of existing buttons; keep data attributes, titles, labels, pressed state, and event handlers unchanged.
- [ ] Run `npm run check`, `cargo test --manifest-path src-tauri/Cargo.toml`, and `git diff --check`; expect no failures except documented ignored live tests.
- [ ] Build `npm run tauri build -- --bundles app`, install and restart `/Applications/focus-bar.app`, configure one Tab icon, and verify Tab plus all three action icons in the installed UI.
