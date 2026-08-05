use serde::Serialize;
use std::env;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::{sleep, timeout};
use url::Url;

const TARGET_TIMEOUT: Duration = Duration::from_secs(8);

const CHROME_SCRIPT: &str = r#"
on run argv
  set targetUrl to item 1 of argv
  tell application "Google Chrome"
    set found to false
    repeat with w in windows
      set tabUrls to URL of every tab of w
      repeat with i from 1 to count of tabUrls
        if item i of tabUrls is targetUrl then
          set active tab index of w to i
          set index of w to 1
          set found to true
          exit repeat
        end if
      end repeat
      if found then exit repeat
    end repeat
    if not found then open location targetUrl
    activate
  end tell
end run
"#;

const VSCODE_SCRIPT: &str = r#"
on run argv
  set targetName to item 1 of argv
  tell application "System Events"
    if not (exists process "Code") then return "NOT_FOUND"
    tell process "Code"
      repeat with w in windows
        if name of w contains targetName then
          set frontmost to true
          perform action "AXRaise" of w
          return "FOUND"
        end if
      end repeat
    end tell
  end tell
  return "NOT_FOUND"
end run
"#;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NavigationErrorCode {
    InvalidTarget,
    ChromeNotInstalled,
    VscodeNotInstalled,
    AutomationPermissionRequired,
    AccessibilityPermissionRequired,
    TargetCommandFailed,
    TargetTimeout,
}

#[derive(Debug, Clone, Serialize)]
pub struct NavigationError {
    pub code: NavigationErrorCode,
    pub message: String,
    pub detail: Option<String>,
}

impl NavigationError {
    pub fn new(
        code: NavigationErrorCode,
        message: impl Into<String>,
        detail: Option<String>,
    ) -> Self {
        Self { code, message: message.into(), detail }
    }
}

struct ScriptSpec {
    script: &'static str,
    args: Vec<String>,
}

pub(crate) fn validate_http_url(value: &str) -> Result<(), NavigationError> {
    let parsed = Url::parse(value).map_err(|error| {
        NavigationError::new(
            NavigationErrorCode::InvalidTarget,
            "Chrome URL is invalid",
            Some(error.to_string()),
        )
    })?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(NavigationError::new(
            NavigationErrorCode::InvalidTarget,
            "Chrome URL must use http or https and include a host",
            None,
        ));
    }
    Ok(())
}

fn chrome_command(url: &str) -> Result<ScriptSpec, NavigationError> {
    validate_http_url(url)?;
    Ok(ScriptSpec { script: CHROME_SCRIPT, args: vec![url.to_string()] })
}

fn safe_relative_file(value: &str) -> bool {
    let path = Path::new(value);
    !path.is_absolute()
        && path.components().all(|component| {
            matches!(component, Component::Normal(_) | Component::CurDir)
        })
}

pub(crate) fn vscode_goto_target(
    workspace: &Path,
    file: Option<&str>,
    line: Option<u32>,
) -> Result<Option<String>, NavigationError> {
    if !workspace.is_absolute() {
        return Err(NavigationError::new(
            NavigationErrorCode::InvalidTarget,
            "VS Code workspace must be absolute",
            None,
        ));
    }
    let Some(file) = file.filter(|value| !value.trim().is_empty()) else {
        if line.is_some() {
            return Err(NavigationError::new(
                NavigationErrorCode::InvalidTarget,
                "A line number requires a file",
                None,
            ));
        }
        return Ok(None);
    };
    if !safe_relative_file(file) {
        return Err(NavigationError::new(
            NavigationErrorCode::InvalidTarget,
            "VS Code file must stay inside the workspace",
            None,
        ));
    }
    if matches!(line, Some(0)) {
        return Err(NavigationError::new(
            NavigationErrorCode::InvalidTarget,
            "VS Code line must be positive",
            None,
        ));
    }
    let mut target = workspace.join(file).to_string_lossy().to_string();
    if let Some(line) = line {
        target.push(':');
        target.push_str(&line.to_string());
    }
    Ok(Some(target))
}

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn resolve_code() -> Result<PathBuf, NavigationError> {
    let mut candidates = Vec::new();
    if let Ok(path) = env::var("PATH") {
        candidates.extend(env::split_paths(&path).map(|directory| directory.join("code")));
    }
    candidates.extend([
        PathBuf::from("/usr/local/bin/code"),
        PathBuf::from("/opt/homebrew/bin/code"),
        PathBuf::from(
            "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
        ),
    ]);
    candidates.into_iter().find(|path| is_executable(path)).ok_or_else(|| {
        NavigationError::new(
            NavigationErrorCode::VscodeNotInstalled,
            "Could not find the official VS Code CLI",
            None,
        )
    })
}

fn classify_automation_failure(detail: &str) -> NavigationErrorCode {
    let lower = detail.to_lowercase();
    if lower.contains("-1743") || lower.contains("not authorized") || lower.contains("not permitted to send apple events") {
        NavigationErrorCode::AutomationPermissionRequired
    } else {
        NavigationErrorCode::TargetCommandFailed
    }
}

fn classify_accessibility_failure(detail: &str) -> NavigationErrorCode {
    let lower = detail.to_lowercase();
    if lower.contains("assistive access")
        || lower.contains("accessibility")
        || lower.contains("辅助访问")
        || lower.contains("-1728")
    {
        NavigationErrorCode::AccessibilityPermissionRequired
    } else {
        NavigationErrorCode::TargetCommandFailed
    }
}

async fn run_command(
    command: &mut Command,
    label: &str,
) -> Result<std::process::Output, NavigationError> {
    run_command_with_timeout(command, label, TARGET_TIMEOUT).await
}

async fn run_command_with_timeout(
    command: &mut Command,
    label: &str,
    duration: Duration,
) -> Result<std::process::Output, NavigationError> {
    command.kill_on_drop(true);
    timeout(duration, command.output())
        .await
        .map_err(|_| {
            NavigationError::new(
                NavigationErrorCode::TargetTimeout,
                format!("{label} timed out"),
                None,
            )
        })?
        .map_err(|error| {
            NavigationError::new(
                NavigationErrorCode::TargetCommandFailed,
                format!("Failed to start {label}"),
                Some(error.to_string()),
            )
        })
}

#[tauri::command]
pub async fn focus_chrome_url(url: String) -> Result<(), NavigationError> {
    if !Path::new("/Applications/Google Chrome.app").exists() {
        return Err(NavigationError::new(
            NavigationErrorCode::ChromeNotInstalled,
            "Google Chrome is not installed",
            None,
        ));
    }
    let spec = chrome_command(&url)?;
    let mut command = Command::new("/usr/bin/osascript");
    command.arg("-e").arg(spec.script).arg("--").args(&spec.args);
    let output = run_command(&mut command, "Google Chrome automation").await?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(NavigationError::new(
        classify_automation_failure(&detail),
        "Could not focus the Chrome target",
        Some(detail),
    ))
}

#[tauri::command]
pub async fn focus_vscode_target(
    workspace: String,
    workspace_name: String,
    file: Option<String>,
    line: Option<u32>,
) -> Result<(), NavigationError> {
    let workspace = PathBuf::from(workspace);
    if !workspace.is_absolute() {
        return Err(NavigationError::new(
            NavigationErrorCode::InvalidTarget,
            "VS Code workspace must be absolute",
            None,
        ));
    }
    let target = vscode_goto_target(&workspace, file.as_deref(), line)?;
    let code = resolve_code()?;
    let name = if workspace_name.trim().is_empty() {
        workspace.file_name().and_then(|value| value.to_str()).unwrap_or("")
    } else {
        workspace_name.trim()
    };

    let mut script_command = Command::new("/usr/bin/osascript");
    script_command.arg("-e").arg(VSCODE_SCRIPT).arg("--").arg(name);
    let output = run_command(&mut script_command, "VS Code window lookup").await?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(NavigationError::new(
            classify_accessibility_failure(&detail),
            "Could not inspect VS Code windows",
            Some(detail),
        ));
    }
    let found = String::from_utf8_lossy(&output.stdout).trim() == "FOUND";
    if !found {
        let mut open_command = Command::new(&code);
        open_command.arg(&workspace);
        let open_output = run_command(&mut open_command, "VS Code workspace open").await?;
        if !open_output.status.success() {
            return Err(NavigationError::new(
                NavigationErrorCode::TargetCommandFailed,
                "Could not open the VS Code workspace",
                Some(String::from_utf8_lossy(&open_output.stderr).trim().to_string()),
            ));
        }
        sleep(Duration::from_millis(250)).await;
    }
    if let Some(target) = target {
        let mut goto_command = Command::new(&code);
        goto_command.arg("--goto").arg(target);
        let goto_output = run_command(&mut goto_command, "VS Code file navigation").await?;
        if !goto_output.status.success() {
            return Err(NavigationError::new(
                NavigationErrorCode::TargetCommandFailed,
                "Could not navigate to the VS Code file",
                Some(String::from_utf8_lossy(&goto_output.stderr).trim().to_string()),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_url_is_an_argument_not_script_source() {
        let hostile = "https://example.com/\"%20&%20do%20shell%20script%20\"bad\"";
        let spec = chrome_command(hostile).unwrap();
        assert!(!spec.script.contains(hostile));
        assert_eq!(spec.args.last().unwrap(), hostile);
    }

    #[test]
    fn chrome_script_reads_tab_urls_once_per_window() {
        let spec = chrome_command("https://example.com").unwrap();
        assert!(spec.script.contains("set tabUrls to URL of every tab of w"));
        assert!(!spec.script.contains("set t to tab i of w"));
    }

    #[test]
    fn vscode_file_target_includes_line() {
        let target = vscode_goto_target(Path::new("/tmp/app"), Some("src/main.ts"), Some(42))
            .unwrap();
        assert_eq!(target.unwrap(), "/tmp/app/src/main.ts:42");
    }

    #[test]
    fn vscode_file_cannot_escape_workspace() {
        assert_eq!(
            vscode_goto_target(Path::new("/tmp/app"), Some("../secret"), None)
                .unwrap_err()
                .code,
            NavigationErrorCode::InvalidTarget
        );
    }

    #[test]
    fn automation_denial_is_typed() {
        assert_eq!(
            classify_automation_failure("Not authorized to send Apple events. (-1743)"),
            NavigationErrorCode::AutomationPermissionRequired
        );
    }

    #[test]
    fn timed_out_command_is_terminated() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let pid_path = std::env::temp_dir().join(format!(
                "focus-bar-timeout-{}-{nonce}.pid",
                std::process::id()
            ));
            let mut command = Command::new("/bin/sh");
            command.arg("-c").arg(format!(
                "echo $$ > '{}'; exec /bin/sleep 5",
                pid_path.display()
            ));

            let error = run_command_with_timeout(
                &mut command,
                "timeout test",
                Duration::from_millis(200),
            )
            .await
            .unwrap_err();
            assert_eq!(error.code, NavigationErrorCode::TargetTimeout);

            sleep(Duration::from_millis(100)).await;
            let pid = std::fs::read_to_string(&pid_path).unwrap();
            let still_running = std::process::Command::new("/bin/kill")
                .arg("-0")
                .arg(pid.trim())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success();
            let _ = std::fs::remove_file(pid_path);
            assert!(!still_running, "timed out child process {pid} is still running");
        });
    }
}
