use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{sleep, timeout};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(15);
static WATCHER_STARTED: AtomicBool = AtomicBool::new(false);
static TRANSCRIPT_EVENT_CACHE: OnceLock<Mutex<HashMap<PathBuf, (u64, Option<(String, String)>)>>> =
    OnceLock::new();

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CmuxErrorCode {
    CliNotFound,
    CmuxNotRunning,
    AccessDenied,
    Timeout,
    InvalidResponse,
    WatcherDisconnected,
}

#[derive(Debug, Clone, Serialize)]
pub struct CmuxError {
    pub code: CmuxErrorCode,
    pub message: String,
    pub detail: Option<String>,
}

impl CmuxError {
    fn new(code: CmuxErrorCode, message: impl Into<String>, detail: Option<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SourceState {
    Ready {
        cli_path: String,
        socket_path: Option<String>,
    },
    Error {
        code: CmuxErrorCode,
        message: String,
        detail: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct CmuxSnapshot {
    pub source: SourceState,
    pub workspaces: Vec<Value>,
    pub notifications: Vec<Value>,
    pub fetched_at: u64,
}

#[derive(Debug, Clone)]
struct RuntimeContext {
    cli_path: PathBuf,
    socket_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
struct CliCommandSpec {
    args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct ProcessInfo {
    pid: u32,
    ppid: u32,
    pgid: i32,
    tty: String,
    state: String,
    command: String,
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn cli_candidates(bundled: Option<&str>, path: Option<&str>, home: Option<&str>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(value) = bundled.filter(|value| !value.trim().is_empty()) {
        candidates.push(PathBuf::from(value));
    }
    if let Some(value) = path {
        candidates.extend(env::split_paths(value).map(|dir| dir.join("cmux")));
    }
    candidates.push(PathBuf::from(
        "/Applications/cmux.app/Contents/Resources/bin/cmux",
    ));
    if let Some(value) = home.filter(|value| !value.trim().is_empty()) {
        candidates
            .push(PathBuf::from(value).join("Applications/cmux.app/Contents/Resources/bin/cmux"));
    }
    candidates
}

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn resolve_cli() -> Result<PathBuf, CmuxError> {
    let bundled = env::var("CMUX_BUNDLED_CLI_PATH").ok();
    let path = env::var("PATH").ok();
    let home = dirs::home_dir();
    cli_candidates(
        bundled.as_deref(),
        path.as_deref(),
        home.as_ref().and_then(|value| value.to_str()),
    )
    .into_iter()
    .find(|candidate| is_executable(candidate))
    .ok_or_else(|| {
        CmuxError::new(
            CmuxErrorCode::CliNotFound,
            "Could not find an executable cmux CLI",
            None,
        )
    })
}

fn resolve_socket() -> Option<PathBuf> {
    if let Ok(value) = env::var("CMUX_SOCKET_PATH") {
        if !value.trim().is_empty() {
            return Some(PathBuf::from(value));
        }
    }
    let last_socket = dirs::home_dir()?.join(".local/state/cmux/last-socket-path");
    let value = std::fs::read_to_string(last_socket).ok()?;
    let candidate = PathBuf::from(value.trim());
    if candidate
        .metadata()
        .map(|metadata| metadata.file_type().is_socket())
        .unwrap_or(false)
    {
        Some(candidate)
    } else {
        None
    }
}

fn runtime_context() -> Result<RuntimeContext, CmuxError> {
    Ok(RuntimeContext {
        cli_path: resolve_cli()?,
        socket_path: resolve_socket(),
    })
}

fn classify_failure(detail: &str) -> CmuxErrorCode {
    let lower = detail.to_lowercase();
    if lower.contains("broken pipe")
        || lower.contains("operation not permitted")
        || lower.contains("permission denied")
        || lower.contains("unauthenticated")
    {
        CmuxErrorCode::AccessDenied
    } else if lower.contains("no such file")
        || lower.contains("failed to connect to socket")
        || lower.contains("connection refused")
    {
        CmuxErrorCode::CmuxNotRunning
    } else {
        CmuxErrorCode::CmuxNotRunning
    }
}

fn parse_json(raw: &str) -> Result<Value, CmuxError> {
    serde_json::from_str(raw).map_err(|error| {
        CmuxError::new(
            CmuxErrorCode::InvalidResponse,
            "cmux returned invalid JSON",
            Some(error.to_string()),
        )
    })
}

fn selected_terminal_surface(response: &Value) -> Option<(String, String, Option<String>)> {
    let surface = response
        .get("surfaces")?
        .as_array()?
        .iter()
        .find(|surface| {
            surface.get("selected").and_then(Value::as_bool) == Some(true)
                && surface.get("type").and_then(Value::as_str) == Some("terminal")
        })?;
    let id = surface.get("id")?.as_str()?.trim();
    let title = surface.get("title")?.as_str()?.trim();
    if id.is_empty() || title.is_empty() {
        return None;
    }
    let tty = surface
        .get("tty")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Some((id.to_string(), title.to_string(), tty))
}

fn surface_title_is_running(title: &str) -> bool {
    let mut characters = title.chars();
    matches!(characters.next(), Some(character) if ('\u{2800}'..='\u{28ff}').contains(&character))
        && characters.next().is_none_or(char::is_whitespace)
}

fn active_progress_line(screen: &str) -> Option<String> {
    screen
        .lines()
        .rev()
        .filter_map(|line| line.trim().strip_prefix('⏺'))
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn parse_process_table(raw: &str) -> Vec<ProcessInfo> {
    raw.lines()
        .filter_map(|line| {
            let columns = line.split_whitespace().collect::<Vec<_>>();
            if columns.len() < 7 {
                return None;
            }
            Some(ProcessInfo {
                pid: columns[0].parse().ok()?,
                ppid: columns[1].parse().ok()?,
                pgid: columns[2].parse().ok()?,
                tty: columns[4].trim_start_matches("/dev/").to_string(),
                state: columns[5].to_string(),
                command: columns[6..].join(" "),
            })
        })
        .collect()
}

fn parse_claude_sessions(raw: &str) -> HashMap<u32, String> {
    raw.lines()
        .filter_map(|line| {
            let mut columns = line.trim().splitn(2, char::is_whitespace);
            let pid = columns.next()?.parse().ok()?;
            let args = columns.next()?.split_whitespace().collect::<Vec<_>>();
            let session = args
                .windows(2)
                .find(|pair| matches!(pair[0], "--resume" | "--session-id"))?
                .get(1)?;
            Some((pid, (*session).to_string()))
        })
        .collect()
}

fn agent_event_from_transcript(raw: &str) -> Option<(String, String)> {
    let mut pending_questions = HashSet::new();
    let mut event = None;
    for line in raw.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(timestamp) = value.get("timestamp").and_then(Value::as_str) else {
            continue;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                let question = value
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .find(|content| {
                        content.get("type").and_then(Value::as_str) == Some("tool_use")
                            && content.get("name").and_then(Value::as_str)
                                == Some("AskUserQuestion")
                    });
                if let Some(question) = question {
                    if let Some(id) = question.get("id").and_then(Value::as_str) {
                        pending_questions.insert(id.to_string());
                    }
                    event = Some(("question".into(), timestamp.into()));
                }
            }
            Some("user") => {
                let results = value
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|content| {
                        (content.get("type").and_then(Value::as_str) == Some("tool_result"))
                            .then(|| content.get("tool_use_id").and_then(Value::as_str))
                            .flatten()
                    })
                    .collect::<Vec<_>>();
                if results
                    .iter()
                    .any(|tool_use_id| pending_questions.remove(*tool_use_id))
                {
                    event = Some(("running".into(), timestamp.into()));
                }
            }
            Some("system")
                if value.get("subtype").and_then(Value::as_str) == Some("turn_duration")
                    && pending_questions.is_empty() =>
            {
                event = Some(("stop".into(), timestamp.into()));
            }
            _ => {}
        }
    }
    event
}

fn process_name(process: &ProcessInfo) -> String {
    Path::new(&process.command)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(&process.command)
        .to_string()
}

fn is_claude_process(process: &ProcessInfo) -> bool {
    matches!(process_name(process).as_str(), "claude" | "claude.exe")
}

fn is_live(process: &ProcessInfo) -> bool {
    !process.state.starts_with('Z')
}

fn background_process_for_claude(
    processes: &[ProcessInfo],
    claude: &ProcessInfo,
) -> Option<String> {
    let background_roots = processes.iter().filter(|process| {
        process.ppid == claude.pid
            && process.pgid > 0
            && process.pgid != claude.pgid
            && is_live(process)
    });
    for root in background_roots {
        let mut subtree = Vec::new();
        let mut stack = vec![root.pid];
        let mut visited = HashSet::new();
        while let Some(pid) = stack.pop() {
            if !visited.insert(pid) {
                continue;
            }
            if let Some(process) = processes.iter().find(|process| process.pid == pid) {
                if is_live(process) {
                    subtree.push(process);
                    stack.extend(
                        processes
                            .iter()
                            .filter(|child| child.ppid == pid)
                            .map(|child| child.pid),
                    );
                }
            }
        }

        let root_name = process_name(root);
        let root_is_shell = matches!(root_name.as_str(), "zsh" | "bash" | "sh");
        if subtree.len() == 1 && root_is_shell {
            continue;
        }
        let meaningful = subtree.iter().find(|process| {
            let name = process_name(process);
            !matches!(
                name.as_str(),
                "zsh" | "bash" | "sh" | "node" | "npm" | "env" | "tee" | "tail"
            ) && !name.starts_with("python")
        });
        let selected = meaningful.or_else(|| subtree.last())?;
        return Some(process_name(selected));
    }
    None
}

fn background_process_for_surface(
    processes: &[ProcessInfo],
    root_pids: &[u32],
    tty: Option<&str>,
) -> Option<String> {
    let rooted_claude = processes
        .iter()
        .filter(|process| {
            root_pids.contains(&process.pid) && is_live(process) && is_claude_process(process)
        })
        .collect::<Vec<_>>();
    let candidates = if rooted_claude.is_empty() {
        let tty = tty
            .map(|value| value.trim_start_matches("/dev/"))
            .unwrap_or_default();
        processes
            .iter()
            .filter(|process| {
                !tty.is_empty()
                    && process.tty == tty
                    && is_live(process)
                    && is_claude_process(process)
            })
            .collect::<Vec<_>>()
    } else {
        rooted_claude
    };

    candidates
        .into_iter()
        .find_map(|claude| background_process_for_claude(processes, claude))
}

fn selected_surface_roots_by_workspace(value: &Value) -> HashMap<String, Vec<u32>> {
    let mut roots = HashMap::new();
    let Some(windows) = value.get("windows").and_then(Value::as_array) else {
        return roots;
    };
    for workspace in windows
        .iter()
        .filter_map(|window| window.get("workspaces").and_then(Value::as_array))
        .flatten()
    {
        let Some(workspace_ref) = workspace.get("ref").and_then(Value::as_str) else {
            continue;
        };
        let Some(panes) = workspace.get("panes").and_then(Value::as_array) else {
            continue;
        };
        let is_selected_terminal = |surface: &&Value| {
            surface.get("selected").and_then(Value::as_bool) == Some(true)
                && surface.get("type").and_then(Value::as_str) == Some("terminal")
        };
        let selected_surface = panes
            .iter()
            .find(|pane| pane.get("focused").and_then(Value::as_bool) == Some(true))
            .and_then(|pane| pane.get("surfaces").and_then(Value::as_array))
            .and_then(|surfaces| surfaces.iter().find(is_selected_terminal))
            .or_else(|| {
                panes
                    .iter()
                    .filter_map(|pane| pane.get("surfaces").and_then(Value::as_array))
                    .flatten()
                    .find(is_selected_terminal)
            });
        let Some(root_pids) = selected_surface
            .and_then(|surface| surface.get("root_pids"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        roots.insert(
            workspace_ref.to_string(),
            root_pids
                .iter()
                .filter_map(Value::as_u64)
                .filter_map(|pid| u32::try_from(pid).ok())
                .collect(),
        );
    }
    roots
}

async fn load_process_table() -> Vec<ProcessInfo> {
    let output = timeout(
        Duration::from_secs(1),
        Command::new("/bin/ps")
            .args(["-axo", "pid=,ppid=,pgid=,tpgid=,tty=,stat=,comm="])
            .output(),
    )
    .await;
    match output {
        Ok(Ok(output)) if output.status.success() => {
            parse_process_table(&String::from_utf8_lossy(&output.stdout))
        }
        _ => Vec::new(),
    }
}

async fn load_claude_sessions(processes: &[ProcessInfo]) -> HashMap<u32, String> {
    let pids = processes
        .iter()
        .filter(|process| is_live(process) && is_claude_process(process))
        .map(|process| process.pid.to_string())
        .collect::<Vec<_>>();
    if pids.is_empty() {
        return HashMap::new();
    }
    let output = timeout(
        Duration::from_secs(1),
        Command::new("/bin/ps")
            .args(["-p", &pids.join(","), "-o", "pid=,args="])
            .output(),
    )
    .await;
    match output {
        Ok(Ok(output)) if output.status.success() => {
            parse_claude_sessions(&String::from_utf8_lossy(&output.stdout))
        }
        _ => HashMap::new(),
    }
}

fn transcript_path(cwd: &str, session_id: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let project = cwd.trim_end_matches('/').replace('/', "-");
    Some(
        home.join(".claude/projects")
            .join(project)
            .join(format!("{session_id}.jsonl")),
    )
}

fn read_file_tail(path: &Path, max_bytes: u64) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut raw = String::new();
    file.read_to_string(&mut raw).ok()?;
    if start > 0 {
        let newline = raw.find('\n')?;
        raw.drain(..=newline);
    }
    Some(raw)
}

fn load_transcript_event(cwd: &str, session_id: &str) -> Option<(String, String)> {
    let path = transcript_path(cwd, session_id)?;
    let len = path.metadata().ok()?.len();
    let cache = TRANSCRIPT_EVENT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().ok()?;
    if let Some((cached_len, event)) = cache.get(&path) {
        if *cached_len == len {
            return event.clone();
        }
    }
    let event = read_file_tail(&path, 2 * 1024 * 1024)
        .as_deref()
        .and_then(agent_event_from_transcript);
    cache.insert(path, (len, event.clone()));
    event
}

async fn load_surface_root_pids(context: &RuntimeContext) -> HashMap<String, Vec<u32>> {
    run_cmux(
        context,
        &["top".into(), "--all".into(), "--json".into()],
        COMMAND_TIMEOUT,
    )
    .await
    .ok()
    .and_then(|raw| parse_json(&raw).ok())
    .map(|value| selected_surface_roots_by_workspace(&value))
    .unwrap_or_default()
}

fn agent_lifecycles_from_value(store: &Value) -> HashMap<String, String> {
    let Some(active) = store
        .get("activeSessionsByWorkspace")
        .and_then(Value::as_object)
    else {
        return HashMap::new();
    };
    let Some(sessions) = store.get("sessions").and_then(Value::as_object) else {
        return HashMap::new();
    };

    active
        .iter()
        .filter_map(|(workspace_id, active_session)| {
            let session_id = active_session.get("sessionId")?.as_str()?;
            let session = sessions.get(session_id)?;
            let lifecycle = session.get("agentLifecycle")?.as_str()?;
            matches!(lifecycle, "running" | "idle" | "needsInput" | "unknown")
                .then(|| (workspace_id.clone(), lifecycle.to_string()))
        })
        .collect()
}

fn load_agent_lifecycles() -> HashMap<String, String> {
    let Some(path) = dirs::home_dir().map(|home| home.join(".cmuxterm/claude-hook-sessions.json"))
    else {
        return HashMap::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .map(|store| agent_lifecycles_from_value(&store))
        .unwrap_or_default()
}

async fn run_cmux(
    context: &RuntimeContext,
    args: &[String],
    deadline: Duration,
) -> Result<String, CmuxError> {
    let mut command = Command::new(&context.cli_path);
    command.args(args);
    if let Some(socket_path) = &context.socket_path {
        command.env("CMUX_SOCKET_PATH", socket_path);
    }
    let output = timeout(deadline, command.output())
        .await
        .map_err(|_| {
            CmuxError::new(
                CmuxErrorCode::Timeout,
                format!("cmux {} timed out", args.join(" ")),
                None,
            )
        })?
        .map_err(|error| {
            CmuxError::new(
                CmuxErrorCode::CmuxNotRunning,
                "Failed to start cmux CLI",
                Some(error.to_string()),
            )
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(CmuxError::new(
            classify_failure(&detail),
            format!("cmux {} failed", args.join(" ")),
            Some(detail),
        ))
    }
}

async fn active_surface_details(
    context: &RuntimeContext,
    workspace_id: &str,
) -> Option<(String, Option<String>, Option<String>)> {
    let raw = run_cmux(
        context,
        &[
            "list-pane-surfaces".into(),
            "--workspace".into(),
            workspace_id.into(),
            "--json".into(),
            "--id-format".into(),
            "both".into(),
        ],
        COMMAND_TIMEOUT,
    )
    .await
    .ok()?;
    let (surface_id, title, tty) = selected_terminal_surface(&parse_json(&raw).ok()?)?;
    let progress = if surface_title_is_running(&title) {
        run_cmux(
            context,
            &[
                "read-screen".into(),
                "--workspace".into(),
                workspace_id.into(),
                "--surface".into(),
                surface_id,
                "--lines".into(),
                "50".into(),
            ],
            COMMAND_TIMEOUT,
        )
        .await
        .ok()
        .and_then(|screen| active_progress_line(&screen))
    } else {
        None
    };
    Some((title, progress, tty))
}

fn error_snapshot(error: CmuxError) -> CmuxSnapshot {
    CmuxSnapshot {
        source: SourceState::Error {
            code: error.code,
            message: error.message,
            detail: error.detail,
        },
        workspaces: Vec::new(),
        notifications: Vec::new(),
        fetched_at: now_millis(),
    }
}

async fn fetch_snapshot_result() -> Result<CmuxSnapshot, CmuxError> {
    let context = runtime_context()?;
    let agent_lifecycles = load_agent_lifecycles();
    let processes = load_process_table().await;
    let claude_sessions = load_claude_sessions(&processes).await;
    run_cmux(&context, &["ping".into()], COMMAND_TIMEOUT).await?;
    let surface_root_pids = load_surface_root_pids(&context).await;

    let windows_raw = run_cmux(
        &context,
        &["list-windows".into(), "--json".into()],
        COMMAND_TIMEOUT,
    )
    .await?;
    let windows = parse_json(&windows_raw)?;
    let windows = windows.as_array().ok_or_else(|| {
        CmuxError::new(
            CmuxErrorCode::InvalidResponse,
            "cmux windows response was not an array",
            None,
        )
    })?;

    let mut workspaces = Vec::new();
    for window in windows {
        let window_id = window.get("id").and_then(Value::as_str).ok_or_else(|| {
            CmuxError::new(
                CmuxErrorCode::InvalidResponse,
                "cmux window did not include an id",
                Some(window.to_string()),
            )
        })?;
        let raw = run_cmux(
            &context,
            &[
                "workspace".into(),
                "list".into(),
                "--json".into(),
                "--id-format".into(),
                "both".into(),
                "--window".into(),
                window_id.into(),
            ],
            COMMAND_TIMEOUT,
        )
        .await?;
        let parsed = parse_json(&raw)?;
        let resolved_window_id = parsed
            .get("window_id")
            .and_then(Value::as_str)
            .unwrap_or(window_id)
            .to_string();
        let items = parsed
            .get("workspaces")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                CmuxError::new(
                    CmuxErrorCode::InvalidResponse,
                    "cmux workspace response did not include workspaces",
                    Some(parsed.to_string()),
                )
            })?;
        for item in items {
            let mut item = item.clone();
            let workspace_id = item.get("id").and_then(Value::as_str).map(str::to_string);
            let workspace_ref = item.get("ref").and_then(Value::as_str).map(str::to_string);
            let root_pids = workspace_ref
                .as_deref()
                .and_then(|workspace_ref| surface_root_pids.get(workspace_ref))
                .map(Vec::as_slice)
                .unwrap_or_default();
            let surface_details = match workspace_id.as_deref() {
                Some(workspace_id) => active_surface_details(&context, workspace_id).await,
                None => None,
            };
            let background_process = surface_details.as_ref().and_then(|(_, _, tty)| {
                background_process_for_surface(&processes, root_pids, tty.as_deref())
            });
            let agent_event = item
                .get("current_directory")
                .and_then(Value::as_str)
                .and_then(|cwd| {
                    root_pids
                        .iter()
                        .find_map(|pid| claude_sessions.get(pid))
                        .and_then(|session_id| load_transcript_event(cwd, session_id))
                });
            if let Some(object) = item.as_object_mut() {
                object.insert(
                    "window_id".into(),
                    Value::String(resolved_window_id.clone()),
                );
                if let Some(lifecycle) = workspace_id
                    .as_deref()
                    .and_then(|workspace_id| agent_lifecycles.get(workspace_id))
                {
                    object.insert("agent_lifecycle".into(), Value::String(lifecycle.clone()));
                }
                if let Some((kind, at)) = agent_event {
                    object.insert("agent_event_kind".into(), Value::String(kind));
                    object.insert("agent_event_at".into(), Value::String(at));
                }
                if let Some((title, progress, _)) = surface_details {
                    object.insert("active_surface_title".into(), Value::String(title));
                    if let Some(progress) = progress {
                        object.insert("active_surface_progress".into(), Value::String(progress));
                    }
                    if let Some(process) = background_process {
                        object.insert("background_shell_process".into(), Value::String(process));
                    }
                }
            }
            workspaces.push(item);
        }
    }

    let notifications_raw = run_cmux(
        &context,
        &["list-notifications".into(), "--json".into()],
        COMMAND_TIMEOUT,
    )
    .await?;
    let notifications = parse_json(&notifications_raw)?
        .as_array()
        .cloned()
        .ok_or_else(|| {
            CmuxError::new(
                CmuxErrorCode::InvalidResponse,
                "cmux notifications response was not an array",
                None,
            )
        })?;

    Ok(CmuxSnapshot {
        source: SourceState::Ready {
            cli_path: context.cli_path.to_string_lossy().to_string(),
            socket_path: context
                .socket_path
                .map(|path| path.to_string_lossy().to_string()),
        },
        workspaces,
        notifications,
        fetched_at: now_millis(),
    })
}

#[derive(Debug, PartialEq, Eq)]
struct ActiveSurfaceContext {
    window_ref: String,
    workspace_ref: String,
    root_pids: Vec<u32>,
}

fn active_surface_context(value: &Value) -> Option<ActiveSurfaceContext> {
    let active = value.get("active")?;
    let active_ref = active.get("surface_ref")?.as_str()?;
    let root_pids = value
        .get("windows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|window| window.get("workspaces").and_then(Value::as_array))
        .flatten()
        .filter_map(|workspace| workspace.get("panes").and_then(Value::as_array))
        .flatten()
        .filter_map(|pane| pane.get("surfaces").and_then(Value::as_array))
        .flatten()
        .find(|surface| surface.get("ref").and_then(Value::as_str) == Some(active_ref))?
        .get("root_pids")?
        .as_array()?
        .into_iter()
        .filter_map(Value::as_u64)
        .filter_map(|pid| u32::try_from(pid).ok())
        .collect();
    Some(ActiveSurfaceContext {
        window_ref: active.get("window_ref")?.as_str()?.to_string(),
        workspace_ref: active.get("workspace_ref")?.as_str()?.to_string(),
        root_pids,
    })
}

pub(crate) async fn current_claude_workspace_id() -> Option<String> {
    let Ok(context) = runtime_context() else {
        return None;
    };
    let active = run_cmux(
        &context,
        &["top".into(), "--json".into()],
        COMMAND_TIMEOUT,
    )
    .await
    .ok()
    .and_then(|raw| parse_json(&raw).ok())
    .and_then(|value| active_surface_context(&value))?;
    if active.root_pids.is_empty() {
        return None;
    }
    let claude_selected = load_process_table().await.iter().any(|process| {
        active.root_pids.contains(&process.pid) && is_live(process) && is_claude_process(process)
    });
    if !claude_selected {
        return None;
    }
    let workspaces = run_cmux(
        &context,
        &[
            "workspace".into(),
            "list".into(),
            "--json".into(),
            "--id-format".into(),
            "both".into(),
            "--window".into(),
            active.window_ref,
        ],
        COMMAND_TIMEOUT,
    )
    .await
    .ok()
    .and_then(|raw| parse_json(&raw).ok())?;
    workspaces
        .get("workspaces")?
        .as_array()?
        .iter()
        .find(|workspace| workspace.get("ref").and_then(Value::as_str) == Some(&active.workspace_ref))
        .and_then(|workspace| workspace.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[tauri::command]
pub async fn fetch_cmux_snapshot() -> CmuxSnapshot {
    match timeout(SNAPSHOT_TIMEOUT, fetch_snapshot_result()).await {
        Ok(Ok(snapshot)) => snapshot,
        Ok(Err(error)) => error_snapshot(error),
        Err(_) => error_snapshot(CmuxError::new(
            CmuxErrorCode::Timeout,
            "Fetching the cmux snapshot timed out",
            None,
        )),
    }
}

fn jump_commands(workspace_ref: &str, workspace_id: &str, window_id: &str) -> Vec<CliCommandSpec> {
    let mut commands = Vec::new();
    if !window_id.is_empty() {
        commands.push(CliCommandSpec {
            args: vec!["focus-window".into(), "--window".into(), window_id.into()],
        });
    }
    let select_target = if workspace_id.is_empty() {
        workspace_ref
    } else {
        workspace_id
    };
    commands.push(CliCommandSpec {
        args: vec!["workspace".into(), "select".into(), select_target.into()],
    });
    if !workspace_id.is_empty() {
        commands.push(CliCommandSpec {
            args: vec![
                "mark-notification-read".into(),
                "--workspace".into(),
                workspace_id.into(),
            ],
        });
    }
    commands
}

#[tauri::command]
pub async fn focus_cmux_workspace(
    workspace_ref: String,
    workspace_id: String,
    window_id: String,
) -> Result<(), CmuxError> {
    let context = runtime_context()?;
    let open_result = timeout(
        COMMAND_TIMEOUT,
        Command::new("/usr/bin/open").args(["-a", "cmux"]).output(),
    )
    .await
    .map_err(|_| CmuxError::new(CmuxErrorCode::Timeout, "Opening cmux timed out", None))?
    .map_err(|error| {
        CmuxError::new(
            CmuxErrorCode::CmuxNotRunning,
            "Failed to open cmux",
            Some(error.to_string()),
        )
    })?;
    if !open_result.status.success() {
        return Err(CmuxError::new(
            CmuxErrorCode::CmuxNotRunning,
            "Could not activate cmux",
            Some(String::from_utf8_lossy(&open_result.stderr).to_string()),
        ));
    }

    for command in jump_commands(&workspace_ref, &workspace_id, &window_id) {
        run_cmux(&context, &command.args, COMMAND_TIMEOUT).await?;
    }
    Ok(())
}

#[tauri::command]
pub fn start_cmux_watcher(app: AppHandle) {
    if WATCHER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut backoff_seconds = 1_u64;
        loop {
            let context = match runtime_context() {
                Ok(context) => context,
                Err(error) => {
                    let _ = app.emit("cmux-watcher-state", &error);
                    sleep(Duration::from_secs(backoff_seconds)).await;
                    backoff_seconds = (backoff_seconds * 2).min(15);
                    continue;
                }
            };

            let mut command = Command::new(&context.cli_path);
            command.args(["events", "--reconnect", "--no-ack", "--no-heartbeat"]);
            if let Some(socket_path) = &context.socket_path {
                command.env("CMUX_SOCKET_PATH", socket_path);
            }
            let child = command.stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
            let mut child = match child {
                Ok(child) => child,
                Err(error) => {
                    let watcher_error = CmuxError::new(
                        CmuxErrorCode::WatcherDisconnected,
                        "Failed to start cmux event watcher",
                        Some(error.to_string()),
                    );
                    let _ = app.emit("cmux-watcher-state", &watcher_error);
                    sleep(Duration::from_secs(backoff_seconds)).await;
                    backoff_seconds = (backoff_seconds * 2).min(15);
                    continue;
                }
            };

            backoff_seconds = 1;
            if let Some(stdout) = child.stdout.take() {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.trim().is_empty() {
                        let _ = app.emit("cmux-state-changed", line);
                    }
                }
            }
            let watcher_error = CmuxError::new(
                CmuxErrorCode::WatcherDisconnected,
                "cmux event watcher disconnected",
                None,
            );
            let _ = app.emit("cmux-watcher-state", &watcher_error);
            let _ = child.kill().await;
            sleep(Duration::from_secs(backoff_seconds)).await;
            backoff_seconds = (backoff_seconds * 2).min(15);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_application_cli_is_a_candidate() {
        let candidates = cli_candidates(None, None, Some("/Users/test"));
        assert!(candidates
            .iter()
            .any(|path| path == Path::new("/Applications/cmux.app/Contents/Resources/bin/cmux")));
        assert!(candidates.iter().any(|path| {
            path == Path::new("/Users/test/Applications/cmux.app/Contents/Resources/bin/cmux")
        }));
    }

    #[test]
    fn broken_pipe_maps_to_access_denied() {
        assert_eq!(
            classify_failure("Failed to write to socket (Broken pipe)"),
            CmuxErrorCode::AccessDenied
        );
    }

    #[test]
    fn missing_socket_maps_to_not_running() {
        assert_eq!(
            classify_failure("Failed to connect to socket: No such file or directory"),
            CmuxErrorCode::CmuxNotRunning
        );
    }

    #[test]
    fn malformed_json_maps_to_invalid_response() {
        assert_eq!(
            parse_json("not-json").unwrap_err().code,
            CmuxErrorCode::InvalidResponse
        );
    }

    #[test]
    fn selected_terminal_surface_includes_id_and_title() {
        let response = serde_json::json!({
            "surfaces": [
                {"id": "surface-5", "selected": false, "title": "yarn serve", "type": "terminal"},
                {"id": "surface-7", "selected": true, "title": "⠂ 支持yarn serve命令动态配置端口", "type": "terminal"}
            ]
        });

        assert_eq!(
            selected_terminal_surface(&response),
            Some((
                "surface-7".into(),
                "⠂ 支持yarn serve命令动态配置端口".into(),
                None
            ))
        );
    }

    #[test]
    fn active_progress_uses_the_last_claude_action() {
        let screen = "⏺ Now delete the header and rebuild.\n\n⏺ Running 1 shell command…\n  ⎿ build (22s)\n\n✽ Canoodling… (4m 26s)";

        assert_eq!(
            active_progress_line(screen),
            Some("Running 1 shell command…".into())
        );
    }

    #[test]
    fn detects_a_live_background_process_group_owned_by_claude() {
        let processes = parse_process_table(
            "12303 20525 12303 12303 ttys000 S+ /usr/local/bin/claude\n\
             92591 12303 12303 12303 ttys000 S clangd\n\
             26041 12303 26041 0 ?? Ss /bin/zsh\n\
             26044 26041 26041 0 ?? S node\n\
             26583 26044 26041 0 ?? S ninja\n\
             40000 39999 40000 40000 ttys001 S yarn",
        );

        assert_eq!(
            background_process_for_surface(&processes, &[], Some("ttys000")),
            Some("ninja".into())
        );
        assert_eq!(
            background_process_for_surface(&processes, &[], Some("ttys001")),
            None
        );
    }

    #[test]
    fn detects_background_process_for_a_restored_surface_with_a_different_tty() {
        let processes = parse_process_table(
            "12303 20525 12303 12303 ttys005 S+ /usr/local/bin/claude\n\
             26041 12303 26041 0 ?? Ss /bin/zsh\n\
             26044 26041 26041 0 ?? S node\n\
             26583 26044 26041 0 ?? S ninja\n\
             27201 20525 27201 27201 ttys010 S /bin/zsh",
        );

        assert_eq!(
            background_process_for_surface(&processes, &[12303, 27201], Some("ttys010")),
            Some("ninja".into())
        );
    }

    #[test]
    fn selected_surface_roots_are_keyed_by_workspace_ref() {
        let response = serde_json::json!({
            "windows": [{
                "workspaces": [{
                    "ref": "workspace:3",
                    "panes": [
                        {
                            "focused": false,
                            "selected_surface_ref": "surface:6",
                            "surfaces": [
                                {"ref": "surface:6", "selected": true, "type": "terminal", "root_pids": [99999]}
                            ]
                        },
                        {
                            "focused": true,
                            "selected_surface_ref": "surface:7",
                            "surfaces": [
                                {"ref": "surface:7", "selected": true, "type": "terminal", "root_pids": [12303, 27201]},
                                {"ref": "surface:8", "selected": false, "type": "terminal", "root_pids": [21673]}
                            ]
                        }
                    ]
                }]
            }]
        });

        let roots = selected_surface_roots_by_workspace(&response);

        assert_eq!(roots.get("workspace:3"), Some(&vec![12303, 27201]));
    }

    #[test]
    fn active_surface_roots_do_not_include_another_surface_in_the_workspace() {
        let response = serde_json::json!({
            "active": {
                "surface_ref": "surface:shell",
                "window_ref": "window:2",
                "workspace_ref": "workspace:4"
            },
            "windows": [{"workspaces": [{"panes": [{"surfaces": [
                {"ref": "surface:claude", "root_pids": [101]},
                {"ref": "surface:shell", "root_pids": [202]}
            ]}]}]}]
        });

        let context = active_surface_context(&response).unwrap();
        assert_eq!(context.root_pids, vec![202]);
        assert_eq!(context.window_ref, "window:2");
        assert_eq!(context.workspace_ref, "workspace:4");
    }

    #[test]
    fn claude_resume_sessions_are_keyed_by_process_id() {
        let output = "22661 claude --resume 94b7bd8f-c7c7-4102-b7a1-fd9474933288 --dangerously-skip-permissions\n\
                      22662 claude --resume 87924453-2ddc-454d-8a00-61588eb9e651 --dangerously-skip-permissions";

        let sessions = parse_claude_sessions(output);

        assert_eq!(
            sessions.get(&22662).map(String::as_str),
            Some("87924453-2ddc-454d-8a00-61588eb9e651")
        );
    }

    #[test]
    fn pending_ask_user_question_is_an_explicit_question_event() {
        let transcript = concat!(
            r#"{"timestamp":"2026-07-14T10:00:00Z","type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"tool-q1","name":"AskUserQuestion"}]}}"#,
            "\n",
            r#"{"timestamp":"2026-07-14T10:00:01Z","type":"system","subtype":"hook_progress"}"#,
        );

        assert_eq!(
            agent_event_from_transcript(transcript),
            Some(("question".into(), "2026-07-14T10:00:00Z".into()))
        );
    }

    #[test]
    fn answered_question_followed_by_turn_end_is_a_stop_event() {
        let transcript = concat!(
            r#"{"timestamp":"2026-07-14T10:00:00Z","type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"tool-q1","name":"AskUserQuestion"}]}}"#,
            "\n",
            r#"{"timestamp":"2026-07-14T10:01:00Z","type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-q1"}]}}"#,
            "\n",
            r#"{"timestamp":"2026-07-14T10:02:00Z","type":"system","subtype":"turn_duration"}"#,
        );

        assert_eq!(
            agent_event_from_transcript(transcript),
            Some(("stop".into(), "2026-07-14T10:02:00Z".into()))
        );
    }

    #[test]
    fn agent_lifecycles_keep_the_newest_session_for_each_workspace() {
        let store = serde_json::json!({
            "activeSessionsByWorkspace": {
                "workspace-1": {"sessionId": "new"},
                "workspace-2": {"sessionId": "running"}
            },
            "sessions": {
                "old": {
                    "workspaceId": "workspace-1",
                    "agentLifecycle": "idle",
                    "updatedAt": 10.0
                },
                "new": {
                    "workspaceId": "workspace-1",
                    "agentLifecycle": "needsInput",
                    "updatedAt": 20.0
                },
                "running": {
                    "workspaceId": "workspace-2",
                    "agentLifecycle": "running",
                    "updatedAt": 15.0
                }
            }
        });

        let lifecycles = agent_lifecycles_from_value(&store);

        assert_eq!(
            lifecycles.get("workspace-1").map(String::as_str),
            Some("needsInput")
        );
        assert_eq!(
            lifecycles.get("workspace-2").map(String::as_str),
            Some("running")
        );
    }

    #[test]
    fn jump_prefers_stable_workspace_id_before_marking_read() {
        let commands = jump_commands("workspace:2", "uuid-2", "window:1");
        assert_eq!(
            commands[0].args,
            vec!["focus-window", "--window", "window:1"]
        );
        assert_eq!(commands[1].args, vec!["workspace", "select", "uuid-2"]);
        assert_eq!(
            commands[2].args,
            vec!["mark-notification-read", "--workspace", "uuid-2"]
        );
    }

    #[test]
    fn jump_falls_back_to_workspace_ref_without_an_id() {
        let commands = jump_commands("workspace:2", "", "window:1");
        assert_eq!(commands[1].args, vec!["workspace", "select", "workspace:2"]);
    }

    #[test]
    #[ignore = "requires a running local cmux with allowAll socket access"]
    fn live_snapshot_is_ready() {
        tauri::async_runtime::block_on(async {
            let snapshot = fetch_cmux_snapshot().await;
            assert!(matches!(snapshot.source, SourceState::Ready { .. }));
            assert!(!snapshot.workspaces.is_empty());
        });
    }

    #[test]
    #[ignore = "requires explicit workspace identifiers and changes cmux focus"]
    fn live_jump_focuses_configured_workspace() {
        tauri::async_runtime::block_on(async {
            let workspace_ref = env::var("FOCUS_BAR_TEST_WORKSPACE_REF").unwrap();
            let workspace_id = env::var("FOCUS_BAR_TEST_WORKSPACE_ID").unwrap();
            let window_id = env::var("FOCUS_BAR_TEST_WINDOW_ID").unwrap();
            focus_cmux_workspace(workspace_ref, workspace_id, window_id)
                .await
                .unwrap();
        });
    }
}
