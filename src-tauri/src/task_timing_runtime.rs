use crate::task_timing::{aggregate_task_timing, TaskStatusSample, TaskTimingSummary};
use crate::task_timing_store::TaskTimingStore;
use std::collections::HashMap;

const MAX_SAMPLE_GAP_MS: u64 = 15_000;

fn current_task_names() -> HashMap<String, String> {
    let Some(path) = dirs::home_dir().map(|home| home.join(".focus.json")) else {
        return HashMap::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|data| data.get("tasks").and_then(serde_json::Value::as_array).cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|task| {
            let id = task.get("id")?.as_str()?.to_string();
            let name = task.get("name")?.as_str()?.to_string();
            Some((id, name))
        })
        .collect()
}

#[tauri::command]
pub fn record_task_status_snapshot(
    tasks: Vec<TaskStatusSample>,
    observed_at: u64,
) -> Result<(), String> {
    let store = TaskTimingStore::new(TaskTimingStore::default_path()?);
    store.initialize()?;
    store.record_snapshot(&tasks, observed_at, MAX_SAMPLE_GAP_MS)
}

#[tauri::command]
pub fn fetch_task_timing_summary(
    range_start: u64,
    range_end: u64,
) -> Result<TaskTimingSummary, String> {
    let store = TaskTimingStore::new(TaskTimingStore::default_path()?);
    store.initialize()?;
    let intervals = store.load_intervals(range_start, range_end)?;
    let mut summary = aggregate_task_timing(&intervals, range_start, range_end);
    let names = current_task_names();
    for task in &mut summary.tasks {
        if let Some(name) = names.get(&task.task_id) {
            task.task_title = name.clone();
        }
    }
    Ok(summary)
}
