import { emitTo, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import chromeIcon from "./assets/tool-icons/chrome.png";
import cmuxIcon from "./assets/tool-icons/cmux.png";
import vscodeIcon from "./assets/tool-icons/vscode.png";
import {
  currentMonitor,
  getCurrentWindow,
  LogicalPosition,
  LogicalSize,
} from "@tauri-apps/api/window";
import { fetchAll, startWatcher } from "./cmux";
import { fetchCodexSnapshot } from "./codex";
import { mergeCodexTasks } from "./codex-view-model";
import {
  createGlobalNoteAutosave,
  saveGlobalNote,
  type GlobalNoteSaveState,
} from "./global-note";
import {
  activateChromeTarget,
  isChromeTargetActive,
  type ActiveChromeTargets,
} from "./chrome-activation";
import { jumpSmart, jumpToChrome, jumpToCmux, jumpToCodex, jumpToVscode } from "./jump";
import { startInactiveHoverTracking } from "./inactive-hover";
import { chromeTargetsFromTask } from "./navigation-config";
import { noteHeightFromDrag } from "./note-resize";
import { tabIconForTask } from "./tab-icon";
import { recordTaskStatusSnapshot, taskStatusSamples } from "./task-timing";
import { findTaskByConfigId, taskListModel } from "./task-groups";
import {
  markTaskCollapsed,
  markTaskViewed,
  readFocusData,
  reconcileCodexConfigs,
  reconcileWorkspaceConfigs,
  writeFocusData,
} from "./store";
import type {
  CmuxSourceErrorCode,
  CmuxSourceState,
  CodexSourceState,
  MergedTask,
  TaskStatus,
} from "./types";
import { STATUS_META } from "./types";
import {
  fallbackRefreshDelay,
  formatRelativeTime,
  mergeWorkspaceTasks,
  sourceMessage,
} from "./view-model";
import { barHeightForContent, barHeightLimits } from "./window-sizing";

const BAR_WIDTH = 200;
const BAR_HEIGHT = 560;

let mergedTasks: MergedTask[] = [];
let cmuxTasks: MergedTask[] = [];
let codexTasks: MergedTask[] = [];
let sourceState: CmuxSourceState | null = null;
let codexSourceState: CodexSourceState | null = null;
let contextMenuTask: MergedTask | null = null;
let stale = false;
let refreshInFlight = false;
let refreshQueued = false;
let eventTimer: ReturnType<typeof setTimeout> | null = null;
let fallbackTimer: ReturnType<typeof setInterval> | null = null;
const activeChromeTargets: ActiveChromeTargets = new Map();
const unlisteners: UnlistenFn[] = [];
let stopInactiveHoverTracking: (() => void) | null = null;
let initialWindowFitDone = false;
let globalNoteStatusTimer: ReturnType<typeof setTimeout> | null = null;
let stopGlobalNoteResizeTracking: (() => void) | null = null;
let inactiveTasksExpanded = false;
const globalNoteAutosave = createGlobalNoteAutosave(saveGlobalNote);

async function positionWindowLeftCenter(height = BAR_HEIGHT) {
  const win = getCurrentWindow();
  const monitor = await currentMonitor();
  if (!monitor) return;
  const logicalX = monitor.workArea.position.x / monitor.scaleFactor;
  const logicalY = monitor.workArea.position.y / monitor.scaleFactor;
  const logicalHeight = monitor.workArea.size.height / monitor.scaleFactor;
  await win.setPosition(new LogicalPosition(
    Math.round(logicalX + 12),
    Math.round(logicalY + Math.max(12, (logicalHeight - height) / 2)),
  ));
}

async function fitWindowToTasks() {
  const monitor = await currentMonitor();
  if (!monitor) return;
  const win = getCurrentWindow();
  const workAreaHeight = monitor.workArea.size.height / monitor.scaleFactor;
  const limits = barHeightLimits(workAreaHeight);
  await win.setMinSize(new LogicalSize(BAR_WIDTH, limits.min));
  await win.setMaxSize(new LogicalSize(BAR_WIDTH, limits.max));
  const taskContainer = document.getElementById("task-container");
  const suggestion = document.getElementById("suggestion");
  const sourceStatus = document.getElementById("source-status");
  const notePanel = document.getElementById("global-note-panel");
  const footerActions = document.getElementById("footer-actions");
  const contentHeight = (taskContainer?.scrollHeight || 0)
    + (suggestion?.offsetHeight || 0)
    + (sourceStatus?.offsetHeight || 0)
    + (notePanel?.offsetHeight || 0)
    + (footerActions?.offsetHeight || 0)
    + 2;
  const height = barHeightForContent(contentHeight, workAreaHeight);
  await win.setSize(new LogicalSize(BAR_WIDTH, height));
  await positionWindowLeftCenter(height);
}

async function gatherSnapshot() {
  let focusData = await readFocusData();
  const [snapshot, codexSnapshot] = await Promise.all([fetchAll(), fetchCodexSnapshot()]);
  let changed = false;
  let nextCmuxTasks: MergedTask[] | null = null;
  let nextCodexTasks: MergedTask[] | null = null;

  if (snapshot.source.status === "ready") {
    const reconciled = reconcileWorkspaceConfigs(snapshot.workspaces, focusData);
    focusData = reconciled.data;
    changed ||= reconciled.changed;
    nextCmuxTasks = mergeWorkspaceTasks(
      snapshot.workspaces,
      snapshot.notifications,
      focusData.tasks,
    );
  }

  if (codexSnapshot.source.status === "ready") {
    const reconciled = reconcileCodexConfigs(codexSnapshot.threads, focusData);
    focusData = reconciled.data;
    changed ||= reconciled.changed;
    nextCodexTasks = mergeCodexTasks(codexSnapshot.threads, focusData.tasks);
  }

  if (changed) await writeFocusData(focusData);

  return {
    cmuxTasks: nextCmuxTasks,
    codexTasks: nextCodexTasks,
    cmuxSource: snapshot.source,
    codexSource: codexSnapshot.source,
  };
}

async function refreshOnce() {
  try {
    const result = await gatherSnapshot();
    sourceState = result.cmuxSource;
    codexSourceState = result.codexSource;
    if (result.cmuxTasks) cmuxTasks = result.cmuxTasks;
    if (result.codexTasks) codexTasks = result.codexTasks;
    mergedTasks = [...cmuxTasks, ...codexTasks];
    stale = sourceState.status === "error" || codexSourceState.status === "error";
    submitTaskStatusSnapshot(result.cmuxTasks, result.codexTasks);
  } catch (error) {
    sourceState = {
      status: "error",
      code: "INVALID_RESPONSE",
      message: "Focus Bar 无法读取 cmux 数据",
      detail: String(error),
    };
    codexSourceState = {
      status: "error",
      message: "Focus Bar 无法读取 Codex 数据",
      detail: String(error),
    };
    stale = mergedTasks.length > 0;
  }
  render();
  if (!initialWindowFitDone) {
    await fitWindowToTasks();
    initialWindowFitDone = true;
  }
  scheduleFallbackRefresh();
}

async function refresh() {
  if (refreshInFlight) {
    refreshQueued = true;
    return;
  }
  refreshInFlight = true;
  try {
    do {
      refreshQueued = false;
      await refreshOnce();
    } while (refreshQueued);
  } finally {
    refreshInFlight = false;
  }
}

function submitTaskStatusSnapshot(
  freshCmuxTasks: MergedTask[] | null,
  freshCodexTasks: MergedTask[] | null,
) {
  if (!freshCmuxTasks && !freshCodexTasks) return;
  const samples = taskStatusSamples(freshCmuxTasks ?? [], freshCodexTasks ?? []);
  if (samples.length === 0) return;
  void recordTaskStatusSnapshot(samples, Date.now()).catch(() => {});
}

function isPendingAttention(status: TaskStatus): boolean {  return status === "needs_action" || status === "needs_review";
}

function computeSuggestion(tasks: MergedTask[]): string {
  const pending = tasks.find((task) => isPendingAttention(task.effectiveStatus));
  if (pending) return `🔴 处理「${pending.title}」`;
  if (tasks.length > 0 && tasks.every((task) => task.effectiveStatus === "executing")) {
    return "🟢 AI 正在执行，可以暂时休息";
  }
  if (tasks.length > 0 && tasks.every((task) => task.effectiveStatus === "idle")) {
    return "⬜ 所有任务空闲";
  }
  return "";
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function renderSourceState() {
  const element = document.getElementById("source-status");
  if (!element) return;
  const messages: string[] = [];
  if (sourceState?.status === "error") messages.push(sourceMessage(sourceState));
  if (codexSourceState?.status === "error") {
    messages.push(codexSourceState.detail || codexSourceState.message);
  }
  if (messages.length === 0) {
    element.className = "";
    element.innerHTML = "";
    return;
  }
  const warning = mergedTasks.length > 0;
  element.className = warning ? "source-warning" : "source-error";
  element.innerHTML = `<span>${escapeHtml(messages.join(" "))}</span><button id="retry-source" type="button">重试</button>`;
  document.getElementById("retry-source")?.addEventListener("click", () => refresh());
}

function renderCard(task: MergedTask): string {
  const meta = STATUS_META[task.effectiveStatus];
  const tabIcon = tabIconForTask(task.config.tab_icon, task.title);
  const folderName = task.directory ? task.directory.split("/").pop() || "" : "";
  const sourceName = task.codex ? "Codex" : "cmux";
  const ports = task.ports.length > 0 ? `:${task.ports.join(", :")}` : "";
  const activityText = [
    task.effectiveStatus === "idle" ? "" : meta.label,
    task.activitySummary,
    formatRelativeTime(task.activityAt),
  ].filter(Boolean).join(" · ");
  const activity = activityText
    ? `<div class="card-activity" title="${escapeHtml(activityText)}">${escapeHtml(activityText)}</div>`
    : "";
  const chromeTargets = chromeTargetsFromTask(task.config);
  const toolIcons: string[] = [];
  if (task.cmux) {
    toolIcons.push(`<button type="button" class="tool-button icon-only" data-tool="cmux" title="跳转 cmux" aria-label="跳转 cmux"><img class="tool-app-icon" src="${cmuxIcon}" alt="" aria-hidden="true" /></button>`);
  }
  if (task.codex) {
    toolIcons.push('<button type="button" class="tool-button icon-only" data-tool="codex" title="跳转 Codex" aria-label="跳转 Codex"><span class="tool-app-icon codex-tool-icon" aria-hidden="true">✦</span></button>');
  }
  if (task.config.vscode) {
    toolIcons.push(`<button type="button" class="tool-button icon-only" data-tool="vscode" title="跳转 VS Code" aria-label="跳转 VS Code"><img class="tool-app-icon" src="${vscodeIcon}" alt="" aria-hidden="true" /></button>`);
  }
  const taskKey = task.config.cmux_workspace_id || task.config.codex_thread_id || task.config.id;
  for (const [targetIndex, target] of chromeTargets.entries()) {
    const label = escapeHtml(target.label || `链接 ${targetIndex + 1}`);
    const active = isChromeTargetActive(
      activeChromeTargets,
      taskKey,
      targetIndex,
      target.url,
    );
    toolIcons.push(`<button type="button" class="tool-button chrome-target${active ? " is-active" : ""}" data-tool="chrome" data-target-index="${targetIndex}" title="打开 ${label}" aria-pressed="${active}"><img class="tool-app-icon" src="${chromeIcon}" alt="" aria-hidden="true" /><span class="tool-label">${label}</span></button>`);
  }
  if (chromeTargets.length === 0 && task.ports.length > 0) {
    toolIcons.push(`<button type="button" class="tool-button chrome-target" data-tool="chrome" title="打开本地预览"><img class="tool-app-icon" src="${chromeIcon}" alt="" aria-hidden="true" /><span class="tool-label">:${task.ports[0]}</span></button>`);
  }
  const note = task.config.note
    ? `<span class="note-badge" title="${escapeHtml(task.config.note)}">📌</span>`
    : "";
  const reason = task.statusReason ? ` title="${escapeHtml(task.statusReason)}"` : "";
  return [
    `<div class="task-card${stale ? " stale" : ""}" data-task-id="${escapeHtml(task.config.id)}"${reason} style="--status-color:${meta.color};--status-bg:${meta.bg}">`,
    '<div class="card-header">',
    `<span class="tab-icon">${escapeHtml(tabIcon)}</span>`,
    `<span class="task-name">${escapeHtml(task.title)}</span>${note}</div>`,
    `<div class="card-meta">${escapeHtml(sourceName)}${folderName ? ` · ${escapeHtml(folderName)}` : ""}${ports ? ` ${escapeHtml(ports)}` : ""}</div>`,
    activity,
    `<div class="card-tools">${toolIcons.join("")}</div>`,
    "</div>",
  ].join("");
}

function renderInactiveSection(model: ReturnType<typeof taskListModel>): string {
  const toggle = model.inactiveToggle;
  if (!toggle) return "";
  const cards = toggle.expanded
    ? `<div class="inactive-task-list" id="inactive-task-list">${model.inactive.map(renderCard).join("")}</div>`
    : "";
  return [
    `<section class="inactive-task-section${toggle.expanded ? " is-expanded" : ""}">`,
    `<button type="button" class="inactive-task-toggle" data-action="toggle-inactive" aria-expanded="${toggle.expanded}" aria-controls="inactive-task-list">`,
    `<span>${escapeHtml(toggle.label)}</span><span class="inactive-task-chevron" aria-hidden="true">›</span>`,
    "</button>",
    cards,
    "</section>",
  ].join("");
}

function render() {
  renderSourceState();
  const container = document.getElementById("task-container");
  const suggestion = document.getElementById("suggestion");
  if (!container || !suggestion) return;

  if (mergedTasks.length === 0) {
    const sourceReady = sourceState?.status === "ready" || codexSourceState?.status === "ready";
    container.innerHTML = `<div class="empty">${sourceReady ? "当前没有可显示的任务" : "无法读取任务数据"}</div>`;
    suggestion.textContent = sourceReady ? "💤 没有正在进行的任务" : "⚠️ 请先恢复任务数据源";
    suggestion.classList.remove("clickable");
    return;
  }

  let model = taskListModel(mergedTasks, inactiveTasksExpanded);
  if (!model.inactiveToggle && inactiveTasksExpanded) {
    inactiveTasksExpanded = false;
    model = taskListModel(mergedTasks, false);
  }
  container.innerHTML = model.current.map(renderCard).join("") + renderInactiveSection(model);
  suggestion.textContent = computeSuggestion(mergedTasks);
  const pendingAttention = mergedTasks.some((task) => isPendingAttention(task.effectiveStatus));
  suggestion.classList.toggle("clickable", pendingAttention);
  attachCardListeners();
}

function attachCardListeners() {
  const container = document.getElementById("task-container");
  if (!container) return;
  container.onclick = (event) => {
    const target = event.target as HTMLElement;
    const inactiveToggle = target.closest<HTMLButtonElement>('[data-action="toggle-inactive"]');
    if (inactiveToggle) {
      inactiveTasksExpanded = !inactiveTasksExpanded;
      render();
      return;
    }
    const card = target.closest(".task-card");
    if (!card) return;
    const task = findTaskByConfigId(
      mergedTasks,
      card.getAttribute("data-task-id") || "",
    );
    if (!task) return;
    const icon = target.closest<HTMLButtonElement>(".tool-button");
    if (icon) {
      event.stopPropagation();
      void handleExplicitJump(task, icon);
    } else {
      void handleSmartJump(task, card as HTMLElement);
    }
  };
  container.oncontextmenu = (event) => {
    const card = (event.target as HTMLElement).closest(".task-card");
    if (!card) return;
    event.preventDefault();
    const task = findTaskByConfigId(
      mergedTasks,
      card.getAttribute("data-task-id") || "",
    );
    if (task) void showContextMenu(task, event.clientX, event.clientY);
  };

  const suggestion = document.getElementById("suggestion");
  if (suggestion) {
    suggestion.onclick = () => {
      const task = mergedTasks.find((item) => isPendingAttention(item.effectiveStatus));
      if (task) void handleSmartJump(task);
    };
  }
}

async function handleSmartJump(task: MergedTask, trigger?: HTMLElement) {
  setJumpBusy(trigger, true);
  showToast(`正在跳转“${task.title}”…`, "progress");
  try {
    await jumpSmart(task);
    await markTaskViewed(task.config.id);
    showToast(`已跳转到“${task.title}”`, "success");
    await refresh();
  } catch (error) {
    showToast(formatError(error), "error");
  } finally {
    setJumpBusy(trigger, false);
  }
}

async function handleExplicitJump(task: MergedTask, trigger: HTMLButtonElement) {
  const tool = trigger.dataset.tool || "";
  const targetIndex = trigger.dataset.targetIndex === undefined
    ? undefined
    : Number(trigger.dataset.targetIndex);
  const chromeTarget = typeof targetIndex === "number"
    ? chromeTargetsFromTask(task.config)[targetIndex]
    : undefined;
  const label = tool === "chrome"
    ? chromeTarget?.label || "本地预览"
    : tool === "vscode" ? "VS Code" : tool === "codex" ? "Codex" : "cmux";
  setJumpBusy(trigger, true);
  showToast(`正在打开“${label}”…`, "progress");
  try {
    if (tool === "cmux") {
      await jumpToCmux(task);
      await markTaskViewed(task.config.id);
    }
    if (tool === "codex") {
      await jumpToCodex(task);
      await markTaskViewed(task.config.id);
    }
    if (tool === "vscode") await jumpToVscode(task);
    if (tool === "chrome") {
      await jumpToChrome(task, chromeTarget, targetIndex);
      if (chromeTarget && typeof targetIndex === "number") {
        activateChromeTarget(
          activeChromeTargets,
          task.config.cmux_workspace_id || task.config.codex_thread_id || task.config.id,
          targetIndex,
          chromeTarget.url,
        );
      }
    }
    showToast(`已打开“${label}”`, "success");
    await refresh();
  } catch (error) {
    showToast(formatError(error), "error");
  } finally {
    setJumpBusy(trigger, false);
  }
}

function setJumpBusy(element: HTMLElement | undefined, busy: boolean) {
  if (!element) return;
  element.classList.toggle("is-loading", busy);
  element.setAttribute("aria-busy", String(busy));
  if (element instanceof HTMLButtonElement) element.disabled = busy;
}

function formatError(error: unknown): string {
  if (typeof error === "object" && error && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

let toastTimer: ReturnType<typeof setTimeout> | null = null;
function showToast(message: string, kind: "" | "progress" | "success" | "error" = "") {
  const toast = document.getElementById("toast");
  if (!toast) return;
  toast.textContent = message;
  toast.className = `show${kind ? ` ${kind}` : ""}`;
  if (toastTimer) clearTimeout(toastTimer);
  if (kind !== "progress") {
    toastTimer = setTimeout(() => toast.classList.remove("show"), 3000);
  }
}

async function showContextMenu(task: MergedTask, x: number, y: number) {
  contextMenuTask = task;
  const menu = document.getElementById("context-menu");
  if (!menu) return;
  const statuses: Array<{ value: TaskStatus; label: string }> = [
    { value: "needs_review", label: "🔴 待处理" },
    { value: "executing", label: "🟢 执行中" },
    { value: "idle", label: "⬜ 空闲" },
  ];
  let html = `<div class="ctx-header">${escapeHtml(task.title)}</div><div class="ctx-section">临时状态:</div>`;
  for (const status of statuses) {
    const active = task.effectiveStatus === status.value
      || (status.value === "needs_review" && task.effectiveStatus === "needs_action")
      ? " active"
      : "";
    html += `<div class="ctx-item${active}" data-action="status" data-value="${status.value}">${status.label}</div>`;
  }
  html += '<div class="ctx-item" data-action="auto">🔄 自动判断</div>';
  if (task.effectiveStatus === "idle") {
    html += '<div class="ctx-item" data-action="collapse">📥 收进较早任务</div>';
  }
  html += '<div class="ctx-divider"></div><div class="ctx-item" data-action="note">📌 记录断点</div>';
  html += '<div class="ctx-item" data-action="configure">⚙️ 配置跳转目标</div>';
  menu.innerHTML = html;
  menu.style.left = "8px";
  menu.style.top = "8px";
  menu.classList.add("show");
  const bounds = menu.getBoundingClientRect();
  menu.style.left = `${Math.min(Math.max(8, x), BAR_WIDTH - bounds.width - 8)}px`;
  menu.style.top = `${Math.min(Math.max(8, y), window.innerHeight - bounds.height - 8)}px`;
  menu.querySelectorAll(".ctx-item").forEach((item) => {
    item.addEventListener("click", () => {
      void handleContextAction(item.getAttribute("data-action"), item.getAttribute("data-value"));
    });
  });
}

async function closeContextMenu() {
  document.getElementById("context-menu")?.classList.remove("show");
}

async function handleContextAction(action: string | null, value: string | null) {
  if (!contextMenuTask) return;
  if (action === "configure") {
    const taskId = contextMenuTask.config.id;
    await closeContextMenu();
    await openSettings(taskId);
    return;
  }
  if (action === "collapse" && contextMenuTask.effectiveStatus === "idle") {
    await markTaskCollapsed(contextMenuTask.config.id);
    await closeContextMenu();
    await refresh();
    return;
  }
  const data = await readFocusData();
  const config = data.tasks.find((task) => task.id === contextMenuTask?.config.id);
  if (config && action === "status" && value) {
    config.manual_status = value as TaskStatus;
    config.manual_status_context_id = config.cmux_workspace_id || config.codex_thread_id || null;
    await writeFocusData(data);
  } else if (config && action === "auto") {
    config.manual_status = null;
    config.manual_status_context_id = null;
    await writeFocusData(data);
  } else if (config && action === "note") {
    const note = prompt("记录断点/疑问:", config.note || "");
    if (note !== null) {
      config.note = note;
      await writeFocusData(data);
    }
  }
  await closeContextMenu();
  await refresh();
}

function scheduleEventRefresh() {
  if (eventTimer) clearTimeout(eventTimer);
  eventTimer = setTimeout(() => void refresh(), 250);
}

function scheduleFallbackRefresh() {
  if (fallbackTimer) clearTimeout(fallbackTimer);
  fallbackTimer = setTimeout(() => {
    fallbackTimer = null;
    void refresh();
  }, fallbackRefreshDelay(mergedTasks, true));
}

async function startEventRefresh() {
  unlisteners.push(await listen("cmux-state-changed", scheduleEventRefresh));
  unlisteners.push(await listen<{ code?: CmuxSourceErrorCode; message?: string; detail?: string }>(
    "cmux-watcher-state",
    (event) => {
      if (event.payload.code === "WATCHER_DISCONNECTED") {
        sourceState = {
          status: "error",
          code: "WATCHER_DISCONNECTED",
          message: event.payload.message || "cmux watcher disconnected",
          detail: event.payload.detail || null,
        };
        stale = mergedTasks.length > 0;
        render();
      }
    },
  ));
  unlisteners.push(await listen("focus-config-changed", () => void refresh()));
  await startWatcher();
  scheduleFallbackRefresh();
}

async function openSettings(taskId?: string) {
  const settings = await WebviewWindow.getByLabel("settings");
  if (!settings) {
    showToast("配置窗口不可用");
    return;
  }
  await settings.show();
  await settings.setFocus();
  await emitTo("settings", "open-settings-for-task", { taskId: taskId || null });
}

async function openStats() {
  try {
    await invoke("open_activity_stats_window");
  } catch (error) {
    showToast(`统计窗口打开失败：${formatError(error)}`, "error");
  }
}

function renderGlobalNoteSaveState(state: GlobalNoteSaveState) {
  const status = document.getElementById("global-note-status");
  if (!status) return;
  if (globalNoteStatusTimer) clearTimeout(globalNoteStatusTimer);
  globalNoteStatusTimer = null;
  status.className = state;
  status.textContent = state === "saved"
    ? "已保存"
    : state === "error" ? "保存失败" : "保存中…";
  if (state === "saved") {
    globalNoteStatusTimer = setTimeout(() => {
      status.textContent = "";
      status.className = "";
      globalNoteStatusTimer = null;
    }, 1_200);
  }
}

async function initializeGlobalNote() {
  const textarea = document.getElementById("global-note") as HTMLTextAreaElement | null;
  if (!textarea) return;
  const data = await readFocusData();
  textarea.value = data.global_note || "";
  textarea.addEventListener("input", () => {
    globalNoteAutosave.schedule(textarea.value, renderGlobalNoteSaveState);
  });
  textarea.addEventListener("blur", () => void globalNoteAutosave.flush());
  const resizer = document.getElementById("global-note-resizer");
  if (resizer) stopGlobalNoteResizeTracking = startGlobalNoteResize(textarea, resizer);
}

function startGlobalNoteResize(textarea: HTMLTextAreaElement, resizer: HTMLElement): () => void {
  let pointerId: number | null = null;
  let startY = 0;
  let startHeight = 0;

  const finish = () => {
    if (pointerId === null) return;
    if (resizer.hasPointerCapture(pointerId)) resizer.releasePointerCapture(pointerId);
    pointerId = null;
    resizer.classList.remove("is-resizing");
    document.getElementById("bar")?.classList.remove("is-note-resizing");
  };
  const onPointerDown = (event: PointerEvent) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    pointerId = event.pointerId;
    startY = event.clientY;
    startHeight = textarea.getBoundingClientRect().height;
    resizer.setPointerCapture(pointerId);
    resizer.classList.add("is-resizing");
    document.getElementById("bar")?.classList.add("is-note-resizing");
  };
  const onPointerMove = (event: PointerEvent) => {
    if (pointerId !== event.pointerId) return;
    event.preventDefault();
    textarea.style.height = `${noteHeightFromDrag(startHeight, event.clientY - startY)}px`;
  };
  const onPointerUp = (event: PointerEvent) => {
    if (pointerId === event.pointerId) finish();
  };

  resizer.addEventListener("pointerdown", onPointerDown);
  resizer.addEventListener("pointermove", onPointerMove);
  resizer.addEventListener("pointerup", onPointerUp);
  resizer.addEventListener("pointercancel", finish);
  return () => {
    finish();
    resizer.removeEventListener("pointerdown", onPointerDown);
    resizer.removeEventListener("pointermove", onPointerMove);
    resizer.removeEventListener("pointerup", onPointerUp);
    resizer.removeEventListener("pointercancel", finish);
  };
}

async function startDrag(event: MouseEvent) {
  const target = event.target as HTMLElement;
  if (target.closest("#global-note-panel")) return;
  if (target.closest(".task-card") || target.closest(".ctx-item") || target.closest("button") || target.closest("#suggestion.clickable") || target.closest("#resize-handle")) return;
  if (event.button === 0) await getCurrentWindow().startDragging();
}

async function startResize(event: MouseEvent) {
  if (event.button !== 0) return;
  event.preventDefault();
  event.stopPropagation();
  initialWindowFitDone = true;
  await getCurrentWindow().startResizeDragging("South");
}

async function main() {
  await positionWindowLeftCenter();
  stopInactiveHoverTracking = await startInactiveHoverTracking();
  await initializeGlobalNote();
  await refresh();
  await startEventRefresh();
  document.getElementById("settings-button")?.addEventListener("click", () => void openSettings());
  document.getElementById("stats-button")?.addEventListener("click", () => void openStats());
  document.getElementById("resize-handle")?.addEventListener("mousedown", (event) => void startResize(event));
  document.getElementById("bar")?.addEventListener("mousedown", (event) => void startDrag(event));
  document.addEventListener("click", (event) => {
    const menu = document.getElementById("context-menu");
    if (menu?.classList.contains("show") && !menu.contains(event.target as Node)) {
      void closeContextMenu();
    }
  });
}

window.addEventListener("beforeunload", () => {
  void globalNoteAutosave.flush();
  stopInactiveHoverTracking?.();
  stopGlobalNoteResizeTracking?.();
  if (eventTimer) clearTimeout(eventTimer);
  if (fallbackTimer) clearInterval(fallbackTimer);
  if (globalNoteStatusTimer) clearTimeout(globalNoteStatusTimer);
  for (const unlisten of unlisteners) unlisten();
});

window.addEventListener("DOMContentLoaded", () => void main());
