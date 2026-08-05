use serde::Serialize;
use serde_json::Value;
use std::env;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{sleep, timeout};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(15);
static WATCHER_STARTED: AtomicBool = AtomicBool::new(false);

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

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn cli_candidates(
    bundled: Option<&str>,
    path: Option<&str>,
    home: Option<&str>,
) -> Vec<PathBuf> {
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
        candidates.push(
            PathBuf::from(value)
                .join("Applications/cmux.app/Contents/Resources/bin/cmux"),
        );
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

fn selected_surface_title(response: &Value) -> Option<String> {
    response
        .get("surfaces")?
        .as_array()?
        .iter()
        .find(|surface| {
            surface.get("selected").and_then(Value::as_bool) == Some(true)
                && surface.get("type").and_then(Value::as_str) == Some("terminal")
        })?
        .get("title")?
        .as_str()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
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

async fn active_surface_title(context: &RuntimeContext, workspace_id: &str) -> Option<String> {
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
    selected_surface_title(&parse_json(&raw).ok()?)
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
    run_cmux(&context, &["ping".into()], COMMAND_TIMEOUT).await?;

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
        let window_id = window
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
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
            let needs_surface_fallback = item.get("latest_submitted_at").is_none_or(Value::is_null);
            let surface_title = if needs_surface_fallback {
                match item.get("id").and_then(Value::as_str) {
                    Some(workspace_id) => active_surface_title(&context, workspace_id).await,
                    None => None,
                }
            } else {
                None
            };
            if let Some(object) = item.as_object_mut() {
                object.insert(
                    "window_id".into(),
                    Value::String(resolved_window_id.clone()),
                );
                if let Some(title) = surface_title {
                    object.insert("active_surface_title".into(), Value::String(title));
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
            command.args([
                "events",
                "--reconnect",
                "--no-ack",
                "--no-heartbeat",
            ]);
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
    fn selected_surface_title_uses_the_selected_terminal() {
        let response = serde_json::json!({
            "surfaces": [
                {"selected": false, "title": "yarn serve", "type": "terminal"},
                {"selected": true, "title": "⠂ 支持yarn serve命令动态配置端口", "type": "terminal"}
            ]
        });

        assert_eq!(
            selected_surface_title(&response),
            Some("⠂ 支持yarn serve命令动态配置端口".into())
        );
    }

    #[test]
    fn jump_prefers_stable_workspace_id_before_marking_read() {
        let commands = jump_commands("workspace:2", "uuid-2", "window:1");
        assert_eq!(commands[0].args, vec!["focus-window", "--window", "window:1"]);
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
