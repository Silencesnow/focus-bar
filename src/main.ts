import { emitTo, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  currentMonitor,
  getCurrentWindow,
  LogicalPosition,
  LogicalSize,
} from "@tauri-apps/api/window";
import { fetchAll, startWatcher } from "./cmux";
import {
  activateChromeTarget,
  isChromeTargetActive,
  type ActiveChromeTargets,
} from "./chrome-activation";
import { jumpSmart, jumpToChrome, jumpToCmux, jumpToVscode } from "./jump";
import { startInactiveHoverTracking } from "./inactive-hover";
import { chromeTargetsFromTask } from "./navigation-config";
import { ensureTaskForCmux, readFocusData, writeFocusData } from "./store";
import type {
  CmuxSourceErrorCode,
  CmuxSourceState,
  MergedTask,
  TaskStatus,
} from "./types";
import { STATUS_META } from "./types";
import { mergeWorkspaceTasks, sourceMessage } from "./view-model";

const BAR_WIDTH = 600;
const BAR_HEIGHT = 140;
const MENU_HEIGHT = 340;

let mergedTasks: MergedTask[] = [];
let sourceState: CmuxSourceState | null = null;
let contextMenuTask: MergedTask | null = null;
let stale = false;
let refreshInFlight = false;
let refreshQueued = false;
let eventTimer: ReturnType<typeof setTimeout> | null = null;
let fallbackTimer: ReturnType<typeof setInterval> | null = null;
const activeChromeTargets: ActiveChromeTargets = new Map();
const unlisteners: UnlistenFn[] = [];
let stopInactiveHoverTracking: (() => void) | null = null;

async function positionWindowTopCenter() {
  const win = getCurrentWindow();
  const monitor = await currentMonitor();
  if (!monitor) return;
  const logicalWidth = monitor.size.width / monitor.scaleFactor;
  await win.setPosition(new LogicalPosition(
    Math.round(logicalWidth / 2 - BAR_WIDTH / 2),
    32,
  ));
}

async function gatherSnapshot() {
  const focusData = await readFocusData();
  const snapshot = await fetchAll();
  if (snapshot.source.status === "error") {
    return { tasks: null, source: snapshot.source };
  }

  for (const workspace of snapshot.workspaces) {
    if (!focusData.tasks.some((task) => task.cmux_workspace_id === workspace.id)) {
      const created = await ensureTaskForCmux(
        workspace.id,
        workspace.title,
        workspace.current_directory,
      );
      focusData.tasks.push(created);
    }
  }

  return {
    tasks: mergeWorkspaceTasks(snapshot.workspaces, snapshot.notifications, focusData.tasks),
    source: snapshot.source,
  };
}

async function refreshOnce() {
  try {
    const result = await gatherSnapshot();
    sourceState = result.source;
    if (result.tasks) {
      mergedTasks = result.tasks;
      stale = false;
    } else {
      stale = mergedTasks.length > 0;
    }
  } catch (error) {
    sourceState = {
      status: "error",
      code: "INVALID_RESPONSE",
      message: "Focus Bar 无法读取 cmux 数据",
      detail: String(error),
    };
    stale = mergedTasks.length > 0;
  }
  render();
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

function computeSuggestion(tasks: MergedTask[]): string {
  const action = tasks.find((task) => task.effectiveStatus === "needs_action");
  if (action) return `🔴 立即处理「${action.title}」`;
  const review = tasks.find((task) => task.effectiveStatus === "needs_review");
  if (review) return `🟡 检查「${review.title}」的结果`;
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
  if (!sourceState || sourceState.status === "ready") {
    element.className = "";
    element.innerHTML = "";
    return;
  }
  const warning = sourceState.code === "WATCHER_DISCONNECTED" && mergedTasks.length > 0;
  element.className = warning ? "source-warning" : "source-error";
  element.innerHTML = `<span>${escapeHtml(sourceMessage(sourceState))}</span><button id="retry-source" type="button">重试</button>`;
  document.getElementById("retry-source")?.addEventListener("click", () => refresh());
}

function renderCard(task: MergedTask, index: number): string {
  const meta = STATUS_META[task.effectiveStatus];
  const folderName = task.directory ? task.directory.split("/").pop() || "" : "";
  const ports = task.ports.length > 0 ? `:${task.ports.join(", :")}` : "";
  const chromeTargets = chromeTargetsFromTask(task.config);
  const toolIcons: string[] = [
    '<button type="button" class="tool-button icon-only" data-tool="cmux" title="跳转 cmux" aria-label="跳转 cmux">📟</button>',
  ];
  if (task.config.vscode) {
    toolIcons.push('<button type="button" class="tool-button icon-only" data-tool="vscode" title="跳转 VS Code" aria-label="跳转 VS Code">📝</button>');
  }
  const taskKey = task.config.cmux_workspace_id || task.config.id;
  for (const [targetIndex, target] of chromeTargets.entries()) {
    const label = escapeHtml(target.label || `链接 ${targetIndex + 1}`);
    const active = isChromeTargetActive(
      activeChromeTargets,
      taskKey,
      targetIndex,
      target.url,
    );
    toolIcons.push(`<button type="button" class="tool-button chrome-target${active ? " is-active" : ""}" data-tool="chrome" data-target-index="${targetIndex}" title="打开 ${label}" aria-pressed="${active}"><span aria-hidden="true">🌐</span><span class="tool-label">${label}</span></button>`);
  }
  if (chromeTargets.length === 0 && task.ports.length > 0) {
    toolIcons.push(`<button type="button" class="tool-button chrome-target" data-tool="chrome" title="打开本地预览"><span aria-hidden="true">🌐</span><span class="tool-label">:${task.ports[0]}</span></button>`);
  }
  const note = task.config.note
    ? `<span class="note-badge" title="${escapeHtml(task.config.note)}">📌</span>`
    : "";
  const reason = task.statusReason ? ` title="${escapeHtml(task.statusReason)}"` : "";
  return [
    `<div class="task-card${stale ? " stale" : ""}" data-index="${index}"${reason} style="--status-color:${meta.color};--status-bg:${meta.bg}">`,
    '<div class="card-header">',
    `<span class="status-dot">${meta.emoji}</span>`,
    `<span class="task-name">${escapeHtml(task.title)}</span>${note}</div>`,
    `<div class="card-meta">${escapeHtml(folderName)}${ports ? ` ${escapeHtml(ports)}` : ""}</div>`,
    `<div class="card-tools">${toolIcons.join("")}</div>`,
    "</div>",
  ].join("");
}

function render() {
  renderSourceState();
  const container = document.getElementById("task-container");
  const suggestion = document.getElementById("suggestion");
  if (!container || !suggestion) return;

  if (mergedTasks.length === 0) {
    const sourceReady = sourceState?.status === "ready";
    container.innerHTML = `<div class="empty">${sourceReady ? "cmux 当前没有 workspace" : "无法读取 cmux workspace"}</div>`;
    suggestion.textContent = sourceReady ? "💤 没有正在进行的任务" : "⚠️ 请先恢复 cmux 数据源";
    suggestion.classList.remove("clickable");
    return;
  }

  container.innerHTML = mergedTasks.map(renderCard).join("");
  suggestion.textContent = computeSuggestion(mergedTasks);
  const actionable = mergedTasks.some((task) => task.effectiveStatus === "needs_action");
  suggestion.classList.toggle("clickable", actionable);
  attachCardListeners();
}

function attachCardListeners() {
  const container = document.getElementById("task-container");
  if (!container) return;
  container.onclick = (event) => {
    const target = event.target as HTMLElement;
    const card = target.closest(".task-card");
    if (!card) return;
    const task = mergedTasks[Number(card.getAttribute("data-index") || "0")];
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
    const task = mergedTasks[Number(card.getAttribute("data-index") || "0")];
    if (task) void showContextMenu(task, event.clientX, event.clientY);
  };

  const suggestion = document.getElementById("suggestion");
  if (suggestion) {
    suggestion.onclick = () => {
      const task = mergedTasks.find((item) => item.effectiveStatus === "needs_action");
      if (task) void handleSmartJump(task);
    };
  }
}

async function handleSmartJump(task: MergedTask, trigger?: HTMLElement) {
  setJumpBusy(trigger, true);
  showToast(`正在跳转“${task.title}”…`, "progress");
  try {
    await jumpSmart(task);
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
    : tool === "vscode" ? "VS Code" : "cmux";
  setJumpBusy(trigger, true);
  showToast(`正在打开“${label}”…`, "progress");
  try {
    if (tool === "cmux") await jumpToCmux(task);
    if (tool === "vscode") await jumpToVscode(task);
    if (tool === "chrome") {
      await jumpToChrome(task, chromeTarget, targetIndex);
      if (chromeTarget && typeof targetIndex === "number") {
        activateChromeTarget(
          activeChromeTargets,
          task.config.cmux_workspace_id || task.config.id,
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
  await getCurrentWindow().setSize(new LogicalSize(BAR_WIDTH, MENU_HEIGHT));
  const menu = document.getElementById("context-menu");
  if (!menu) return;
  const statuses: Array<{ value: TaskStatus; label: string }> = [
    { value: "needs_action", label: "🔴 需要处理" },
    { value: "needs_review", label: "🟡 待检查" },
    { value: "executing", label: "🟢 执行中" },
    { value: "idle", label: "⬜ 空闲" },
  ];
  let html = `<div class="ctx-header">${escapeHtml(task.title)}</div><div class="ctx-section">临时状态:</div>`;
  for (const status of statuses) {
    const active = task.effectiveStatus === status.value ? " active" : "";
    html += `<div class="ctx-item${active}" data-action="status" data-value="${status.value}">${status.label}</div>`;
  }
  html += '<div class="ctx-item" data-action="auto">🔄 自动判断</div>';
  html += '<div class="ctx-divider"></div><div class="ctx-item" data-action="note">📌 记录断点</div>';
  html += '<div class="ctx-item" data-action="configure">⚙️ 配置跳转目标</div>';
  menu.innerHTML = html;
  menu.style.left = `${Math.min(x, BAR_WIDTH - 190)}px`;
  menu.style.top = `${Math.max(y, BAR_HEIGHT - 8)}px`;
  menu.classList.add("show");
  menu.querySelectorAll(".ctx-item").forEach((item) => {
    item.addEventListener("click", () => {
      void handleContextAction(item.getAttribute("data-action"), item.getAttribute("data-value"));
    });
  });
}

async function closeContextMenu() {
  document.getElementById("context-menu")?.classList.remove("show");
  await getCurrentWindow().setSize(new LogicalSize(BAR_WIDTH, BAR_HEIGHT));
}

async function handleContextAction(action: string | null, value: string | null) {
  if (!contextMenuTask) return;
  if (action === "configure") {
    const taskId = contextMenuTask.config.id;
    await closeContextMenu();
    await openSettings(taskId);
    return;
  }
  const data = await readFocusData();
  const config = data.tasks.find((task) => task.id === contextMenuTask?.config.id);
  if (config && action === "status" && value) {
    config.manual_status = value as TaskStatus;
    await writeFocusData(data);
  } else if (config && action === "auto") {
    config.manual_status = null;
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
  fallbackTimer = setInterval(() => void refresh(), 30_000);
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

async function startDrag(event: MouseEvent) {
  const target = event.target as HTMLElement;
  if (target.closest(".task-card") || target.closest(".ctx-item") || target.closest("button") || target.closest("#suggestion.clickable")) return;
  if (event.button === 0) await getCurrentWindow().startDragging();
}

async function main() {
  await positionWindowTopCenter();
  stopInactiveHoverTracking = await startInactiveHoverTracking();
  await refresh();
  await startEventRefresh();
  document.getElementById("settings-button")?.addEventListener("click", () => void openSettings());
  document.getElementById("bar")?.addEventListener("mousedown", (event) => void startDrag(event));
  document.addEventListener("click", (event) => {
    const menu = document.getElementById("context-menu");
    if (menu?.classList.contains("show") && !menu.contains(event.target as Node)) {
      void closeContextMenu();
    }
  });
}

window.addEventListener("beforeunload", () => {
  stopInactiveHoverTracking?.();
  if (eventTimer) clearTimeout(eventTimer);
  if (fallbackTimer) clearInterval(fallbackTimer);
  for (const unlisten of unlisteners) unlisten();
});

window.addEventListener("DOMContentLoaded", () => void main());
