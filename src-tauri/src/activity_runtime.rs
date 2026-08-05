use crate::activity_store::ActivityStore;
use crate::activity_tracker::{
    aggregate_segments, ActivitySource, ActivitySummary, ActivityType, Confidence, Observation,
    SegmentTracker,
};
use objc2_app_kit::NSWorkspace;
use serde::Deserialize;
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
use tokio::process::Command;
use tokio::time::{interval, timeout, MissedTickBehavior};

const IDLE_TIMEOUT_MS: u64 = 90_000;
const MIN_SEGMENT_MS: u64 = 2_000;
const CONTEXT_REFRESH_MS: u64 = 2_000;
const CHECKPOINT_MS: u64 = 10_000;
const MAX_PENDING_SEGMENTS: usize = 1_000;
const RETRY_BATCH_SIZE: usize = 4;
const CODEX_HINT_TTL_MS: u64 = 5 * 60_000;
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(1);
#[derive(Clone)]
struct CodexThreadHint {
    thread_id: String,
    expires_at: u64,
}
static LAST_CODEX_THREAD: OnceLock<Mutex<Option<CodexThreadHint>>> = OnceLock::new();

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state_id: i32, event_type: u32) -> f64;
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActivityTask {
    pub id: String,
    pub name: String,
    pub cmux_workspace_id: Option<String>,
    pub codex_thread_id: Option<String>,
    pub chrome: Option<OneOrMany<ChromeTarget>>,
    pub vscode: Option<VscodeTarget>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChromeTarget {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VscodeTarget {
    pub workspace: String,
    pub workspace_name: Option<String>,
}

#[derive(Deserialize)]
struct ActivityConfig {
    #[serde(default)]
    tasks: Vec<ActivityTask>,
}

pub fn parse_activity_tasks(raw: &str) -> Result<Vec<ActivityTask>, String> {
    serde_json::from_str::<ActivityConfig>(raw)
        .map(|config| config.tasks)
        .map_err(|error| format!("无法解析任务配置: {error}"))
}

fn chrome_targets(task: &ActivityTask) -> Vec<&ChromeTarget> {
    match task.chrome.as_ref() {
        Some(OneOrMany::One(target)) => vec![target],
        Some(OneOrMany::Many(targets)) => targets.iter().collect(),
        None => Vec::new(),
    }
}

pub fn match_chrome_task<'a>(tasks: &'a [ActivityTask], current_url: &str) -> Option<(&'a ActivityTask, usize)> {
    tasks.iter().find_map(|task| {
        chrome_targets(task)
            .iter()
            .position(|target| crate::navigation::chrome_urls_match(&target.url, current_url))
            .map(|index| (task, index))
    })
}

fn workspace_name(task: &ActivityTask) -> Option<String> {
    let vscode = task.vscode.as_ref()?;
    vscode.workspace_name.clone().or_else(|| {
        std::path::Path::new(&vscode.workspace)
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::to_string)
    })
}

pub fn match_vscode_task<'a>(tasks: &'a [ActivityTask], window_title: &str) -> Option<&'a ActivityTask> {
    let parts = window_title.split(" — ").map(str::trim).collect::<Vec<_>>();
    tasks.iter().find(|task| {
        workspace_name(task).is_some_and(|name| {
            parts.iter().any(|part| *part == name || *part == format!("{name} (Workspace)"))
        })
    })
}

pub fn classify_activity(source: ActivitySource, editable: bool, keyboard_recent: bool) -> ActivityType {
    match source {
        ActivitySource::Cmux | ActivitySource::Codex if editable && keyboard_recent => ActivityType::AiInput,
        ActivitySource::Cmux | ActivitySource::Codex => ActivityType::AiReading,
        ActivitySource::Chrome => ActivityType::BrowserReview,
        ActivitySource::Vscode if editable && keyboard_recent => ActivityType::CodeEditing,
        ActivitySource::Vscode => ActivityType::CodeReading,
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn seconds_since_event(event_type: u32) -> f64 {
    // Combined session state. Only event timing is read; event payloads and key values are never captured.
    unsafe { CGEventSourceSecondsSinceLastEventType(0, event_type) }
}

fn input_timing(now: u64) -> (u64, bool) {
    const USER_EVENTS: [u32; 9] = [1, 3, 5, 6, 7, 10, 22, 25, 27];
    let idle_seconds = USER_EVENTS
        .iter()
        .map(|event_type| seconds_since_event(*event_type))
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .fold(f64::INFINITY, f64::min);
    let keyboard_seconds = seconds_since_event(10);
    let idle_ms = if idle_seconds.is_finite() { (idle_seconds * 1_000.0) as u64 } else { IDLE_TIMEOUT_MS };
    (now.saturating_sub(idle_ms), keyboard_seconds.is_finite() && keyboard_seconds <= 2.5)
}

fn frontmost_bundle_id() -> Option<String> {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .and_then(|application| application.bundleIdentifier())
        .map(|identifier| identifier.to_string())
}

fn source_for_bundle(bundle_id: &str) -> Option<ActivitySource> {
    match bundle_id {
        "com.cmuxterm.app" => Some(ActivitySource::Cmux),
        "com.openai.codex" => Some(ActivitySource::Codex),
        "com.google.Chrome" => Some(ActivitySource::Chrome),
        "com.microsoft.VSCode" => Some(ActivitySource::Vscode),
        _ => None,
    }
}

fn read_tasks() -> Vec<ActivityTask> {
    let path = dirs::home_dir().map(|home| home.join(".focus.json"));
    path.and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|raw| parse_activity_tasks(&raw).ok())
        .unwrap_or_default()
}

async fn script_output(script: &str) -> Option<String> {
    let result = timeout(
        SCRIPT_TIMEOUT,
        Command::new("/usr/bin/osascript").args(["-e", script]).output(),
    )
    .await
    .ok()?
    .ok()?;
    result.status.success().then(|| String::from_utf8_lossy(&result.stdout).trim().to_string())
}

async fn focused_element_is_editable(source: ActivitySource) -> Option<bool> {
    let script = r#"tell application "System Events"
set p to first application process whose frontmost is true
try
  set e to value of attribute "AXFocusedUIElement" of p
  set r to value of attribute "AXRole" of e
  set d to ""
  try
    set d to value of attribute "AXDescription" of e
  end try
  return r & "|" & d
on error
  return ""
end try
end tell"#;
    let Some(value) = script_output(script).await else {
        return None;
    };
    if value.is_empty() {
        return None;
    }
    let lower = value.to_lowercase();
    let editable = value.starts_with("AXTextArea") || value.starts_with("AXTextField");
    if source == ActivitySource::Vscode {
        Some(editable && !lower.contains("search") && !lower.contains("command") && !lower.contains("quick input"))
    } else {
        Some(editable)
    }
}

async fn editable_when_needed(source: ActivitySource, keyboard_recent: bool) -> Option<bool> {
    if keyboard_recent {
        focused_element_is_editable(source).await
    } else {
        Some(false)
    }
}

async fn chrome_active_url() -> Option<String> {
    script_output(r#"tell application "Google Chrome"
if (count of windows) is 0 then return ""
return URL of active tab of front window
end tell"#).await.filter(|value| !value.is_empty())
}

async fn vscode_window_title() -> Option<String> {
    script_output(r#"tell application "System Events"
if not (exists process "Code") then return ""
tell process "Code"
  if (count of windows) is 0 then return ""
  return name of front window
end tell
end tell"#).await.filter(|value| !value.is_empty())
}

fn unassigned(source: ActivitySource, activity_type: ActivityType) -> Observation {
    Observation {
        source,
        activity_type,
        task_id: None,
        task_title: Some("未归属".into()),
        confidence: Confidence::Low,
        context_key: None,
    }
}

async fn resolve_observation(source: ActivitySource, keyboard_recent: bool) -> Option<Observation> {
    let tasks = read_tasks();
    match source {
        ActivitySource::Cmux => {
            let workspace_id = crate::cmux_runtime::current_claude_workspace_id().await?;
            let editable = editable_when_needed(source, keyboard_recent).await?;
            let activity_type = classify_activity(source, editable, keyboard_recent);
            let task = tasks.iter().find(|task| task.cmux_workspace_id.as_deref() == Some(&workspace_id));
            Some(task.map_or_else(
                || unassigned(source, activity_type),
                |task| Observation {
                    source,
                    activity_type,
                    task_id: Some(task.id.clone()),
                    task_title: Some(task.name.clone()),
                    confidence: Confidence::High,
                    context_key: Some(workspace_id),
                },
            ))
        }
        ActivitySource::Codex => {
            let editable = editable_when_needed(source, keyboard_recent).await?;
            let activity_type = classify_activity(source, editable, keyboard_recent);
            let thread_id = {
                let mut hint = LAST_CODEX_THREAD
                    .get_or_init(|| Mutex::new(None))
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if hint.as_ref().is_some_and(|hint| hint.expires_at <= now_millis()) {
                    *hint = None;
                }
                hint.as_ref().map(|hint| hint.thread_id.clone())
            };
            let task = thread_id.as_deref().and_then(|thread_id| {
                tasks.iter().find(|task| task.codex_thread_id.as_deref() == Some(thread_id))
            });
            Some(task.map_or_else(
                || unassigned(source, activity_type),
                |task| Observation {
                    source,
                    activity_type,
                    task_id: Some(task.id.clone()),
                    task_title: Some(task.name.clone()),
                    confidence: Confidence::Medium,
                    context_key: thread_id,
                },
            ))
        }
        ActivitySource::Chrome => {
            let url = chrome_active_url().await?;
            let (task, target_index) = match_chrome_task(&tasks, &url)?;
            Some(Observation {
                source,
                activity_type: ActivityType::BrowserReview,
                task_id: Some(task.id.clone()),
                task_title: Some(task.name.clone()),
                confidence: Confidence::High,
                context_key: Some(format!("chrome:{}", target_index)),
            })
        }
        ActivitySource::Vscode => {
            let title = vscode_window_title().await?;
            let editable = editable_when_needed(source, keyboard_recent).await?;
            let activity_type = classify_activity(source, editable, keyboard_recent);
            let task = match_vscode_task(&tasks, &title);
            Some(task.map_or_else(
                || unassigned(source, activity_type),
                |task| Observation {
                    source,
                    activity_type,
                    task_id: Some(task.id.clone()),
                    task_title: Some(task.name.clone()),
                    confidence: Confidence::High,
                    context_key: task.vscode.as_ref().map(|target| target.workspace.clone()),
                },
            ))
        }
    }
}

pub fn start(_app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Ok(path) = ActivityStore::default_path() else { return };
        let store = ActivityStore::new(path);
        if store.initialize().is_err() { return }
        let mut tracker = SegmentTracker::new(IDLE_TIMEOUT_MS, MIN_SEGMENT_MS);
        let mut ticker = interval(Duration::from_secs(1));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut last_bundle = String::new();
        let mut last_context_refresh = 0;
        let mut last_checkpoint = now_millis();
        let mut last_tick_at = last_checkpoint;
        let mut observation = None;
        let mut pending_segments = VecDeque::new();

        loop {
            ticker.tick().await;
            let now = now_millis();
            if now.saturating_sub(last_tick_at) > 5_000 {
                pending_segments.extend(tracker.close(last_tick_at));
                observation = None;
            }
            last_tick_at = now;
            let (last_user_activity_at, keyboard_recent) = input_timing(now);
            let bundle = frontmost_bundle_id().unwrap_or_default();
            if last_bundle == "com.openai.codex" && bundle != "com.openai.codex" {
                *LAST_CODEX_THREAD
                    .get_or_init(|| Mutex::new(None))
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
            }
            let source = source_for_bundle(&bundle);
            if bundle != last_bundle || now.saturating_sub(last_context_refresh) >= CONTEXT_REFRESH_MS {
                observation = match source {
                    Some(source) => resolve_observation(source, keyboard_recent).await,
                    None => None,
                };
                last_bundle = bundle;
                last_context_refresh = now;
            }

            let mut closed = tracker.update(observation.clone(), last_user_activity_at, now);
            if now.saturating_sub(last_checkpoint) >= CHECKPOINT_MS && tracker.current().is_some() {
                closed.extend(tracker.close(now));
                tracker.update(observation.clone(), last_user_activity_at, now);
                last_checkpoint = now;
            }
            pending_segments.extend(closed);
            while pending_segments.len() > MAX_PENDING_SEGMENTS {
                pending_segments.pop_front();
            }
            for _ in 0..RETRY_BATCH_SIZE {
                let Some(segment) = pending_segments.front().cloned() else {
                    break;
                };
                let writer = store.clone();
                let result = tokio::task::spawn_blocking(move || writer.insert(&segment)).await;
                if !matches!(result, Ok(Ok(()))) {
                    break;
                }
                pending_segments.pop_front();
            }
        }
    });
}

#[tauri::command]
pub fn note_codex_thread_opened(thread_id: String) {
    *LAST_CODEX_THREAD
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(CodexThreadHint {
            thread_id,
            expires_at: now_millis().saturating_add(CODEX_HINT_TTL_MS),
        });
}

#[tauri::command]
pub fn fetch_activity_summary(range_start: u64, range_end: u64) -> Result<ActivitySummary, String> {
    let store = ActivityStore::new(ActivityStore::default_path()?);
    store.initialize()?;
    let segments = store.load_segments(range_start, range_end)?;
    let mut summary = aggregate_segments(&segments, range_start, range_end);
    let tasks = read_tasks();
    for task in &mut summary.tasks {
        if let Some(current) = task
            .task_id
            .as_deref()
            .and_then(|id| tasks.iter().find(|candidate| candidate.id == id))
        {
            task.task_title = current.name.clone();
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tasks() -> Vec<ActivityTask> {
        parse_activity_tasks(r#"{
          "tasks": [
            {
              "id": "cmux-a",
              "name": "Ling Design",
              "cmux_workspace_id": "workspace-a",
              "chrome": [{"label":"MR", "url":"https://git.example.com/group/repo/merges/7342/files"}],
              "vscode": {"workspace":"/work/ling-design", "workspace_name":"ling-design"}
            },
            {
              "id": "codex-a",
              "name": "Codex Task",
              "codex_thread_id": "thread-a"
            }
          ]
        }"#).unwrap()
    }

    #[test]
    fn parses_task_context_keys_from_focus_config() {
        let tasks = tasks();
        assert_eq!(tasks[0].cmux_workspace_id.as_deref(), Some("workspace-a"));
        assert_eq!(tasks[1].codex_thread_id.as_deref(), Some("thread-a"));
    }

    #[test]
    fn chrome_merge_views_map_to_the_same_task() {
        let tasks = tasks();
        let task = match_chrome_task(
            &tasks,
            "https://git.example.com/group/repo/merges/7342/commits#note",
        ).unwrap();
        assert_eq!(task.0.id, "cmux-a");
        assert_eq!(task.1, 0);
    }

    #[test]
    fn chrome_subpaths_use_the_same_boundary_prefix_as_navigation() {
        let tasks = parse_activity_tasks(r#"{
          "tasks": [{
            "id": "review-a",
            "name": "Review A",
            "chrome": {"url":"https://git.example.com/group/repo/review"}
          }]
        }"#).unwrap();

        assert_eq!(
            match_chrome_task(&tasks, "https://git.example.com/group/repo/review/files").unwrap().0.id,
            "review-a"
        );
        assert!(match_chrome_task(&tasks, "https://git.example.com/group/repo/reviewer").is_none());
    }

    #[test]
    fn vscode_window_title_matches_workspace_boundaries() {
        assert_eq!(match_vscode_task(&tasks(), "index.ts — ling-design — Visual Studio Code").unwrap().id, "cmux-a");
        assert!(match_vscode_task(&tasks(), "index.ts — ling-design-B — Visual Studio Code").is_none());
    }

    #[test]
    fn keyboard_activity_only_counts_as_input_in_an_editable_target() {
        assert_eq!(classify_activity(ActivitySource::Codex, true, true), ActivityType::AiInput);
        assert_eq!(classify_activity(ActivitySource::Codex, false, true), ActivityType::AiReading);
        assert_eq!(classify_activity(ActivitySource::Vscode, true, true), ActivityType::CodeEditing);
        assert_eq!(classify_activity(ActivitySource::Vscode, false, true), ActivityType::CodeReading);
    }
}
