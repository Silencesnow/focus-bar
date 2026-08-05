import { invoke } from "@tauri-apps/api/core";
import { rangeForPeriod, type ActivityPeriod } from "./activity-summary";
import { formatRangeStart, renderTaskTimingSummary, type TaskTimingSummary } from "./task-timing-summary";

let period: ActivityPeriod = "today";
let refreshTimer: ReturnType<typeof setInterval> | null = null;

function escapeHtml(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

async function refresh() {
  const { start, end } = rangeForPeriod(period);
  document.getElementById("range-start")!.textContent = formatRangeStart(period, start);
  try {
    const summary = await invoke<TaskTimingSummary>("fetch_task_timing_summary", {
      rangeStart: start,
      rangeEnd: end,
    });
    document.getElementById("stats-content")!.innerHTML = renderTaskTimingSummary(summary, period);
    document.getElementById("last-updated")!.textContent = `更新于 ${new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
  } catch (error) {
    document.getElementById("stats-content")!.innerHTML = `<div class="empty-state error"><h2>统计读取失败</h2><p>${escapeHtml(String(error))}</p><button id="retry" type="button">重试</button></div>`;
    document.getElementById("retry")?.addEventListener("click", () => void refresh());
  }
}

function setPeriod(next: ActivityPeriod) {
  period = next;
  document.querySelectorAll<HTMLButtonElement>("[data-period]").forEach((button) => button.classList.toggle("active", button.dataset.period === period));
  void refresh();
}

window.addEventListener("DOMContentLoaded", () => {
  document.querySelectorAll<HTMLButtonElement>("[data-period]").forEach((button) => button.addEventListener("click", () => setPeriod(button.dataset.period as ActivityPeriod)));
  void refresh();
  refreshTimer = setInterval(() => void refresh(), 10_000);
});
window.addEventListener("focus", () => void refresh());
window.addEventListener("beforeunload", () => { if (refreshTimer) clearInterval(refreshTimer); });
