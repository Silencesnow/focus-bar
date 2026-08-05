use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command;

static TRANSCRIPT_CACHE: OnceLock<Mutex<HashMap<PathBuf, (u64, TranscriptState)>>> =
    OnceLock::new();

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexLifecycle {
    Executing,
    NeedsInput,
    Completed,
    Failed,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptState {
    pub lifecycle: CodexLifecycle,
    pub activity_at: Option<String>,
    pub latest_message: Option<String>,
    pending_input_call: Option<String>,
}

impl Default for TranscriptState {
    fn default() -> Self {
        Self {
            lifecycle: CodexLifecycle::Idle,
            activity_at: None,
            latest_message: None,
            pending_input_call: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexThread {
    pub id: String,
    pub title: String,
    pub cwd: String,
    pub lifecycle: CodexLifecycle,
    pub updated_at: u64,
    pub activity_at: Option<u64>,
    pub latest_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CodexSourceState {
    Ready {
        state_path: String,
    },
    Error {
        message: String,
        detail: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexSnapshot {
    pub source: CodexSourceState,
    pub threads: Vec<CodexThread>,
    pub fetched_at: u64,
}

#[derive(Debug, Deserialize)]
struct ThreadRow {
    id: String,
    title: String,
    cwd: String,
    rollout_path: Option<String>,
    #[serde(default)]
    updated_at_ms: u64,
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn timestamp(value: &Value) -> Option<String> {
    value.get("timestamp")?.as_str().map(str::to_string)
}

fn nonempty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn apply_transcript_line(state: &mut TranscriptState, line: &str) {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return;
    };
    let Some(payload) = value.get("payload") else {
        return;
    };
    let event_type = payload
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let event_at = timestamp(&value);

    if value.get("type").and_then(Value::as_str) == Some("event_msg") {
        match event_type {
            "task_started" => {
                state.lifecycle = CodexLifecycle::Executing;
                state.pending_input_call = None;
                state.activity_at = event_at;
            }
            "task_complete" => {
                state.lifecycle = CodexLifecycle::Completed;
                state.pending_input_call = None;
                state.activity_at = event_at;
                if let Some(message) =
                    nonempty(payload.get("last_agent_message").and_then(Value::as_str))
                {
                    state.latest_message = Some(message);
                }
            }
            "turn_aborted" => {
                state.lifecycle = CodexLifecycle::Idle;
                state.pending_input_call = None;
                state.activity_at = event_at;
            }
            "error" => {
                state.lifecycle = CodexLifecycle::Failed;
                state.pending_input_call = None;
                state.activity_at = event_at;
            }
            "agent_message" => {
                if let Some(message) = nonempty(payload.get("message").and_then(Value::as_str)) {
                    state.latest_message = Some(message);
                    state.activity_at = event_at;
                }
            }
            _ => {}
        }
        return;
    }

    if value.get("type").and_then(Value::as_str) != Some("response_item") {
        return;
    }
    if event_type == "custom_tool_call"
        && payload.get("name").and_then(Value::as_str) == Some("request_user_input")
    {
        state.pending_input_call = payload
            .get("call_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        state.lifecycle = CodexLifecycle::NeedsInput;
        state.activity_at = event_at;
    } else if event_type == "custom_tool_call_output" {
        let call_id = payload.get("call_id").and_then(Value::as_str);
        if state.pending_input_call.as_deref() == call_id {
            state.pending_input_call = None;
            state.lifecycle = CodexLifecycle::Executing;
            state.activity_at = event_at;
        }
    }
}

pub fn parse_transcript(raw: &str) -> TranscriptState {
    let mut state = TranscriptState::default();
    for line in raw.lines() {
        apply_transcript_line(&mut state, line);
    }
    state
}

fn cached_transcript_state(path: &Path) -> TranscriptState {
    let Ok(metadata) = path.metadata() else {
        return TranscriptState::default();
    };
    let length = metadata.len();
    let cache = TRANSCRIPT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (offset, mut state) = cache
        .get(path)
        .filter(|(cached_length, _)| *cached_length <= length)
        .cloned()
        .unwrap_or((0, TranscriptState::default()));
    if offset == length {
        return state;
    }
    let Ok(mut file) = std::fs::File::open(path) else {
        return state;
    };
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return state;
    }
    let mut appended = String::new();
    if file.read_to_string(&mut appended).is_err() {
        return state;
    }
    for line in appended.lines() {
        apply_transcript_line(&mut state, line);
    }
    cache.insert(path.to_path_buf(), (length, state.clone()));
    state
}

fn file_modified_millis(path: &Path) -> Option<u64> {
    path.metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

fn state_db_path() -> Option<PathBuf> {
    let codex_home = dirs::home_dir()?.join(".codex");
    std::fs::read_dir(codex_home)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            let version = name
                .strip_prefix("state_")?
                .strip_suffix(".sqlite")?
                .parse::<u32>()
                .ok()?;
            Some((version, entry.path()))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, path)| path)
}

async fn fetch_snapshot_result() -> Result<CodexSnapshot, String> {
    let state_path = state_db_path().ok_or_else(|| "找不到 Codex 本地状态数据库".to_string())?;
    let query = "select id, title, cwd, rollout_path, updated_at_ms from threads where archived = 0 and source in ('vscode', 'cli') order by recency_at desc limit 100;";
    let output = Command::new("/usr/bin/sqlite3")
        .args(["-json", state_path.to_string_lossy().as_ref(), query])
        .output()
        .await
        .map_err(|error| format!("无法启动 sqlite3: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let rows: Vec<ThreadRow> = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("无法解析 Codex 任务列表: {error}"))?;
    let threads = rows
        .into_iter()
        .map(|row| {
            let rollout_path = row.rollout_path.as_deref().map(Path::new);
            let state = rollout_path
                .map(cached_transcript_state)
                .unwrap_or_default();
            let activity_at = rollout_path
                .and_then(file_modified_millis)
                .or(Some(row.updated_at_ms));
            CodexThread {
                id: row.id,
                title: row
                    .title
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
                cwd: row.cwd,
                lifecycle: state.lifecycle,
                updated_at: row.updated_at_ms,
                activity_at,
                latest_message: state.latest_message,
            }
        })
        .collect();
    Ok(CodexSnapshot {
        source: CodexSourceState::Ready {
            state_path: state_path.to_string_lossy().to_string(),
        },
        threads,
        fetched_at: now_millis(),
    })
}

#[tauri::command]
pub async fn fetch_codex_snapshot() -> CodexSnapshot {
    match fetch_snapshot_result().await {
        Ok(snapshot) => snapshot,
        Err(error) => CodexSnapshot {
            source: CodexSourceState::Error {
                message: "Focus Bar 无法读取 Codex 数据".to_string(),
                detail: Some(error),
            },
            threads: Vec::new(),
            fetched_at: now_millis(),
        },
    }
}
