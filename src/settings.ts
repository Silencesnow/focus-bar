import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { fetchAll } from "./cmux";
import {
  formFromTask,
  formsEqual,
  navigationErrorMessage,
  normalizeNavigationForm,
  validateNavigationForm,
  type NavigationForm,
} from "./navigation-config";
import { ensureTaskForCmux, readFocusData } from "./store";
import type { CmuxWorkspace, NavigationError, TaskConfig } from "./types";
import { sourceMessage } from "./view-model";

interface SettingsTask { workspace: CmuxWorkspace; config: TaskConfig }

const fieldIds = {
  name: "task-name", chromeUrl: "chrome-url", vscodeWorkspaceName: "vscode-name",
  vscodeWorkspace: "vscode-workspace", vscodeFile: "vscode-file", vscodeLine: "vscode-line",
} as const;

let tasks: SettingsTask[] = [];
let selectedTaskId: string | null = null;
let savedForm: NavigationForm | null = null;
let requestedTaskId: string | null = null;

function input(id: string): HTMLInputElement { return document.getElementById(id) as HTMLInputElement; }
function currentForm(): NavigationForm {
  return Object.fromEntries(Object.entries(fieldIds).map(([key, id]) => [key, input(id).value])) as unknown as NavigationForm;
}
function setForm(form: NavigationForm) {
  for (const [key, id] of Object.entries(fieldIds)) input(id).value = form[key as keyof NavigationForm];
  savedForm = { ...form };
  updateDirtyState();
}
function isDirty(): boolean { return !!savedForm && !formsEqual(savedForm, currentForm()); }
function updateDirtyState() {
  const save = document.getElementById("save-settings") as HTMLButtonElement;
  save.disabled = !savedForm || !isDirty();
}
function setStatus(message: string, kind: "" | "error" | "success" = "") {
  const element = document.getElementById("settings-status")!;
  element.textContent = message;
  element.className = kind;
}
function formatError(error: unknown): string {
  if (typeof error === "object" && error && "code" in error) return navigationErrorMessage(error as NavigationError);
  return String(error);
}

function renderTaskList() {
  const list = document.getElementById("workspace-list")!;
  list.innerHTML = tasks.map(({ workspace, config }) => `
    <button type="button" class="workspace-item${config.id === selectedTaskId ? " selected" : ""}" data-task-id="${config.id}" role="option">
      <span class="workspace-title">${escapeHtml(config.name || workspace.title)}</span>
      <span class="workspace-dir">${escapeHtml(workspace.current_directory)}</span>
    </button>`).join("");
  list.querySelectorAll<HTMLButtonElement>(".workspace-item").forEach((button) => {
    button.onclick = () => void selectTask(button.dataset.taskId || "");
  });
}

function escapeHtml(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

async function selectTask(taskId: string) {
  if (taskId === selectedTaskId) return;
  if (isDirty() && !confirm("当前修改尚未保存，确定切换任务吗？")) return;
  const task = tasks.find((item) => item.config.id === taskId);
  if (!task) return;
  selectedTaskId = taskId;
  document.getElementById("settings-empty")!.hidden = true;
  document.getElementById("navigation-form")!.hidden = false;
  setForm(formFromTask(task.config));
  setStatus("");
  renderTaskList();
}

async function loadTasks() {
  const snapshot = await fetchAll();
  if (snapshot.source.status === "error") {
    setStatus(sourceMessage(snapshot.source), "error");
    return;
  }
  const focusData = await readFocusData();
  tasks = [];
  for (const workspace of snapshot.workspaces) {
    let config = focusData.tasks.find((item) => item.cmux_workspace_id === workspace.id);
    if (!config) config = await ensureTaskForCmux(workspace.id, workspace.title, workspace.current_directory);
    tasks.push({ workspace, config });
  }
  renderTaskList();
  const initial = requestedTaskId && tasks.some((item) => item.config.id === requestedTaskId)
    ? requestedTaskId : tasks[0]?.config.id;
  if (initial) await selectTask(initial);
}

async function saveCurrent() {
  if (!selectedTaskId) return;
  const form = currentForm();
  const errors = validateNavigationForm(form);
  if (errors.length) { setStatus(errors[0], "error"); return; }
  const normalized = normalizeNavigationForm(form);
  try {
    const updated = await invoke<TaskConfig>("save_task_navigation", {
      taskId: selectedTaskId, name: normalized.name,
      chrome: normalized.chrome, vscode: normalized.vscode,
    });
    const task = tasks.find((item) => item.config.id === selectedTaskId);
    if (task) task.config = updated;
    setForm(formFromTask(updated));
    renderTaskList();
    setStatus("已保存", "success");
  } catch (error) { setStatus(formatError(error), "error"); }
}

async function testChrome() {
  const form = currentForm();
  const normalized = normalizeNavigationForm(form);
  if (!normalized.chrome) { setStatus("请先输入 Chrome URL", "error"); return; }
  const errors = validateNavigationForm({ ...form, vscodeWorkspaceName: "", vscodeWorkspace: "", vscodeFile: "", vscodeLine: "" });
  if (errors.length) { setStatus(errors[0], "error"); return; }
  try { await invoke("focus_chrome_url", { url: normalized.chrome.url }); setStatus("Chrome 跳转成功", "success"); }
  catch (error) { setStatus(formatError(error), "error"); }
}

async function testVscode() {
  const form = currentForm();
  const normalized = normalizeNavigationForm(form);
  if (!normalized.vscode) { setStatus("请先输入 VS Code workspace", "error"); return; }
  const errors = validateNavigationForm({ ...form, chromeUrl: "" });
  if (errors.length) { setStatus(errors[0], "error"); return; }
  try {
    const target = normalized.vscode;
    await invoke("focus_vscode_target", {
      workspace: target.workspace, workspaceName: target.workspace_name || "",
      file: target.file || null, line: target.line || null,
    });
    setStatus("VS Code 跳转成功", "success");
  } catch (error) { setStatus(formatError(error), "error"); }
}

function clearChrome() { input("chrome-url").value = ""; updateDirtyState(); void saveCurrent(); }
function clearVscode() {
  for (const id of ["vscode-name", "vscode-workspace", "vscode-file", "vscode-line"]) input(id).value = "";
  updateDirtyState(); void saveCurrent();
}

async function main() {
  document.querySelectorAll("input").forEach((element) => element.addEventListener("input", updateDirtyState));
  document.getElementById("navigation-form")!.addEventListener("submit", (event) => { event.preventDefault(); void saveCurrent(); });
  document.getElementById("test-chrome")!.addEventListener("click", () => void testChrome());
  document.getElementById("test-vscode")!.addEventListener("click", () => void testVscode());
  document.getElementById("clear-chrome")!.addEventListener("click", clearChrome);
  document.getElementById("clear-vscode")!.addEventListener("click", clearVscode);
  await listen<{ taskId?: string }>("open-settings-for-task", (event) => {
    requestedTaskId = event.payload.taskId || null;
    if (requestedTaskId) void selectTask(requestedTaskId);
  });
  await getCurrentWindow().onCloseRequested(async (event) => {
    event.preventDefault();
    if (!isDirty() || confirm("尚有未保存修改，确定关闭吗？")) await getCurrentWindow().hide();
  });
  await loadTasks();
}

window.addEventListener("DOMContentLoaded", () => void main());
