use serde::Serialize;
use std::env;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::{sleep, timeout};
use url::Url;

const TARGET_TIMEOUT: Duration = Duration::from_secs(8);
const CHROME_EXECUTABLE: &str =
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

#[derive(Debug, Clone, PartialEq)]
struct ChromeProcess {
    pid: u32,
    command: String,
}

fn parse_chrome_processes(output: &str) -> Vec<ChromeProcess> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let split = line.find(char::is_whitespace)?;
            let pid = line[..split].parse().ok()?;
            let command = line[split..].trim();
            if !command.starts_with(CHROME_EXECUTABLE)
                || command
                    .as_bytes()
                    .get(CHROME_EXECUTABLE.len())
                    .is_some_and(|character| !character.is_ascii_whitespace())
                || command.contains("--remote-debugging-port")
            {
                return None;
            }
            Some(ChromeProcess { pid, command: command.into() })
        })
        .collect()
}

fn select_chrome_pid(processes: &[ChromeProcess], window_order: &[u32]) -> Option<u32> {
    let eligible: std::collections::HashSet<u32> =
        processes.iter().map(|process| process.pid).collect();
    window_order
        .iter()
        .copied()
        .find(|pid| eligible.contains(pid))
        .or_else(|| processes.iter().map(|process| process.pid).min())
}

#[cfg(target_os = "macos")]
fn chrome_window_order() -> Vec<u32> {
    use core_foundation::array::CFArray;
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::window::{
        copy_window_info, kCGNullWindowID, kCGWindowListExcludeDesktopElements,
        kCGWindowListOptionOnScreenOnly, kCGWindowOwnerPID,
    };

    let options = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
    let Some(raw) = copy_window_info(options, kCGNullWindowID) else {
        return vec![];
    };
    let windows: CFArray<CFDictionary<CFString, CFType>> =
        unsafe { CFArray::wrap_under_get_rule(raw.as_concrete_TypeRef()) };
    let pid_key = unsafe { CFString::wrap_under_get_rule(kCGWindowOwnerPID) };
    let mut seen = std::collections::HashSet::new();

    windows
        .iter()
        .filter_map(|window| {
            let value = window.find(&pid_key)?;
            let pid = value.downcast::<CFNumber>()?.to_i64()? as u32;
            seen.insert(pid).then_some(pid)
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn chrome_window_order() -> Vec<u32> {
    vec![]
}

async fn discover_chrome_processes() -> Result<Vec<ChromeProcess>, NavigationError> {
    let mut command = Command::new("/bin/ps");
    command.args(["-axo", "pid=,command="]);
    let output = run_command(&mut command, "Chrome process discovery").await?;
    if !output.status.success() {
        return Err(NavigationError::new(
            NavigationErrorCode::TargetCommandFailed,
            "Could not inspect Chrome processes",
            Some(String::from_utf8_lossy(&output.stderr).trim().into()),
        ));
    }
    Ok(parse_chrome_processes(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

const CHROME_SCRIPT: &str = r##"
use framework "Foundation"
use framework "AppKit"
use framework "ScriptingBridge"

on matchesNavigationTarget(candidateUrl, matchPrefix)
  if candidateUrl is matchPrefix then return true
  set candidateText to current application's NSString's stringWithString:candidateUrl
  if (candidateText's hasPrefix:(matchPrefix & "/")) as boolean then return true
  if (candidateText's hasPrefix:(matchPrefix & "#")) as boolean then return true
  if (candidateText's hasPrefix:(matchPrefix & "?")) as boolean then return true
  return false
end matchesNavigationTarget

on run argv
  set targetPid to (item 1 of argv) as integer
  set targetUrl to item 2 of argv
  set matchPrefix to item 3 of argv
  set chromeApp to current application's SBApplication's applicationWithProcessIdentifier:targetPid
  set runningApp to current application's NSRunningApplication's runningApplicationWithProcessIdentifier:targetPid
  set chromeWindows to chromeApp's valueForKey:"windows"
  set windowCount to count of chromeWindows

  if windowCount > 0 then
    repeat with windowIndex from 0 to (windowCount - 1)
      set chromeWindow to chromeWindows's objectAtIndex:windowIndex
      set chromeTabs to chromeWindow's valueForKey:"tabs"
      set tabUrls to chromeTabs's valueForKey:"URL"
      set tabCount to count of tabUrls
      if tabCount > 0 then
        repeat with tabIndex from 0 to (tabCount - 1)
          if my matchesNavigationTarget((tabUrls's objectAtIndex:tabIndex) as text, matchPrefix) then
            chromeWindow's setValue:(tabIndex + 1) forKey:"activeTabIndex"
            chromeWindow's setValue:1 forKey:"index"
            runningApp's activateWithOptions:3
            return "FOUND"
          end if
        end repeat
      end if
    end repeat
  end if
  return "NOT_FOUND"
end run
"##;

const VSCODE_SCRIPT: &str = r#"
on matchesWorkspaceTitle(candidateName, targetName)
  set paddedCandidate to " — " & candidateName & " — "
  set targetVariants to {targetName, targetName & " (Workspace)"}
  repeat with targetVariant in targetVariants
    if paddedCandidate contains (" — " & (targetVariant as text) & " — ") then return true
  end repeat
  return false
end matchesWorkspaceTitle

on run argv
  if (count of argv) is 3 then
    if item 1 of argv is "__FOCUS_BAR_MATCH_TITLE__" then
      if my matchesWorkspaceTitle(item 2 of argv, item 3 of argv) then return "MATCH"
      return "NO_MATCH"
    end if
  end if

  set targetName to item 1 of argv
  tell application "System Events"
    if not (exists process "Code") then return "NOT_FOUND"
    tell process "Code"
      repeat with w in windows
        if my matchesWorkspaceTitle(name of w, targetName) then
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

fn parse_chrome_url(value: &str) -> Result<Url, NavigationError> {
    let parsed = Url::parse(value).map_err(|error| {
        NavigationError::new(
            NavigationErrorCode::InvalidTarget,
            "Chrome URL is invalid",
            Some(error.to_string()),
        )
    })?;
    let is_web_url =
        matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some();
    let is_local_file_url = parsed.scheme() == "file"
        && parsed.host_str().is_none()
        && parsed.path().starts_with('/');
    if !is_web_url && !is_local_file_url {
        return Err(NavigationError::new(
            NavigationErrorCode::InvalidTarget,
            "Chrome URL must use http, https, or a local file URL",
            None,
        ));
    }
    Ok(parsed)
}

pub(crate) fn validate_chrome_url(value: &str) -> Result<(), NavigationError> {
    parse_chrome_url(value).map(|_| ())
}

pub(crate) fn chrome_match_prefix(value: &str) -> Result<String, NavigationError> {
    let mut parsed = parse_chrome_url(value)?;
    parsed.set_query(None);
    parsed.set_fragment(None);

    if matches!(parsed.scheme(), "http" | "https") {
        let segments = parsed
            .path_segments()
            .map(|segments| segments.collect::<Vec<_>>())
            .unwrap_or_default();
        if let Some(merge_index) = segments.iter().position(|segment| *segment == "merges") {
            if segments.get(merge_index + 1).is_some_and(|id| {
                !id.is_empty() && id.chars().all(|character| character.is_ascii_digit())
            }) {
                parsed.set_path(&format!("/{}", segments[..=merge_index + 1].join("/")));
            }
        }
    }

    if parsed.path().len() > 1 && parsed.path().ends_with('/') {
        let normalized_path = parsed.path().trim_end_matches('/').to_string();
        parsed.set_path(&normalized_path);
    }

    Ok(parsed.to_string())
}

pub(crate) fn chrome_urls_match(configured: &str, candidate: &str) -> bool {
    let Ok(configured) = chrome_match_prefix(configured) else {
        return false;
    };
    let Ok(candidate) = chrome_match_prefix(candidate) else {
        return false;
    };
    candidate == configured
        || candidate.strip_prefix(&configured).is_some_and(|suffix| {
            suffix.starts_with('/') || suffix.starts_with('?') || suffix.starts_with('#')
        })
}

fn chrome_command(pid: u32, url: &str) -> Result<ScriptSpec, NavigationError> {
    let match_prefix = chrome_match_prefix(url)?;
    Ok(ScriptSpec {
        script: CHROME_SCRIPT,
        args: vec![pid.to_string(), url.to_string(), match_prefix],
    })
}

fn chrome_open_args(url: &str) -> Result<Vec<String>, NavigationError> {
    validate_chrome_url(url)?;
    Ok(vec!["--new-tab".into(), url.into()])
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

fn vscode_window_was_found(status_success: bool, response: &str) -> bool {
    status_success && response.trim() == "FOUND"
}

fn vscode_open_target(workspace: &Path, workspace_name: &str) -> PathBuf {
    if workspace.extension().is_some_and(|extension| extension == "code-workspace") {
        return workspace.to_path_buf();
    }
    let named_workspace = workspace.join(format!("{workspace_name}.code-workspace"));
    if named_workspace.is_file() {
        named_workspace
    } else {
        workspace.to_path_buf()
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
    let processes = discover_chrome_processes().await?;
    let pid = select_chrome_pid(&processes, &chrome_window_order()).ok_or_else(|| {
        NavigationError::new(
            NavigationErrorCode::ChromeNotInstalled,
            "No ordinary Google Chrome process is running",
            Some("Remote-debugging Chrome processes are intentionally ignored".into()),
        )
    })?;
    let spec = chrome_command(pid, &url)?;
    let mut command = Command::new("/usr/bin/osascript");
    command
        .arg("-l")
        .arg("AppleScript")
        .arg("-e")
        .arg(spec.script)
        .arg("--")
        .args(&spec.args);
    let output = run_command(&mut command, "Google Chrome automation").await?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(NavigationError::new(
            classify_automation_failure(&detail),
            "Could not focus the Chrome target",
            Some(detail),
        ));
    }
    match String::from_utf8_lossy(&output.stdout).trim() {
        "FOUND" => Ok(()),
        "NOT_FOUND" => {
            let mut open_command = Command::new(CHROME_EXECUTABLE);
            open_command.args(chrome_open_args(&url)?);
            let open_output = run_command(&mut open_command, "Google Chrome new tab").await?;
            if open_output.status.success() {
                Ok(())
            } else {
                Err(NavigationError::new(
                    NavigationErrorCode::TargetCommandFailed,
                    "Could not open the Chrome target",
                    Some(String::from_utf8_lossy(&open_output.stderr).trim().into()),
                ))
            }
        }
        response => Err(NavigationError::new(
            NavigationErrorCode::TargetCommandFailed,
            "Chrome automation returned an unexpected response",
            Some(response.into()),
        )),
    }
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
    let open_target = vscode_open_target(&workspace, name);

    let mut script_command = Command::new("/usr/bin/osascript");
    script_command.arg("-e").arg(VSCODE_SCRIPT).arg("--").arg(name);
    let lookup = run_command(&mut script_command, "VS Code window lookup").await;
    let found = match &lookup {
        Ok(output) => {
            if !output.status.success() {
                let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
                eprintln!(
                    "VS Code window lookup failed ({:?}): {detail}",
                    classify_accessibility_failure(&detail)
                );
            }
            vscode_window_was_found(
                output.status.success(),
                &String::from_utf8_lossy(&output.stdout),
            )
        }
        Err(error) => {
            eprintln!(
                "VS Code window lookup failed ({:?}): {}",
                error.code, error.message
            );
            false
        }
    };
    if !found {
        let mut open_command = Command::new(&code);
        open_command.arg(&open_target);
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

    fn chrome_process(pid: u32) -> ChromeProcess {
        ChromeProcess { pid, command: CHROME_EXECUTABLE.into() }
    }

    #[test]
    fn chrome_process_parser_excludes_remote_debugging() {
        let output = "712 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome\n\
                      1899 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome --remote-debugging-port=9222 --user-data-dir=/tmp/debug\n\
                      1905 /Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper --type=gpu-process";
        assert_eq!(
            parse_chrome_processes(output),
            vec![ChromeProcess { pid: 712, command: CHROME_EXECUTABLE.into() }]
        );
    }

    #[test]
    fn frontmost_eligible_window_wins() {
        let processes = vec![chrome_process(100), chrome_process(200)];
        assert_eq!(select_chrome_pid(&processes, &[999, 200, 100]), Some(200));
    }

    #[test]
    fn hidden_windows_fall_back_to_lowest_pid() {
        let processes = vec![chrome_process(200), chrome_process(100)];
        assert_eq!(select_chrome_pid(&processes, &[]), Some(100));
    }

    #[test]
    fn debug_only_processes_produce_no_target() {
        let output = "1899 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome --remote-debugging-port=9222";
        assert_eq!(select_chrome_pid(&parse_chrome_processes(output), &[]), None);
    }

    #[test]
    fn chrome_pid_and_url_are_arguments_not_script_source() {
        let hostile = "https://example.com/\"%20&%20do%20shell%20script%20\"bad\"";
        let spec = chrome_command(712, hostile).unwrap();
        assert!(!spec.script.contains(hostile));
        assert_eq!(
            spec.args,
            vec!["712".to_string(), hostile.to_string(), chrome_match_prefix(hostile).unwrap()]
        );
        assert!(spec.script.contains("applicationWithProcessIdentifier"));
    }

    #[test]
    fn chrome_match_prefix_ignores_query_and_fragment() {
        assert_eq!(
            chrome_match_prefix("https://example.com/review?mode=files#comment-12").unwrap(),
            "https://example.com/review"
        );
    }

    #[test]
    fn local_file_url_is_a_valid_chrome_target() {
        let url = "file:///Users/test/Documents/preview.html";

        assert_eq!(chrome_match_prefix(url).unwrap(), url);
    }

    #[test]
    fn local_file_url_can_be_opened_in_a_new_chrome_tab() {
        let url = "file:///Users/test/Documents/preview.html";

        assert_eq!(chrome_open_args(url).unwrap(), vec!["--new-tab", url]);
    }

    #[test]
    fn chrome_match_prefix_collapses_merge_views() {
        let expected = "http://xingyun.jd.com/codingRoot/ling/ling-design/merges/7342";
        for url in [
            expected,
            "http://xingyun.jd.com/codingRoot/ling/ling-design/merges/7342/files",
            "http://xingyun.jd.com/codingRoot/ling/ling-design/merges/7342/commits",
            "http://xingyun.jd.com/codingRoot/ling/ling-design/merges/7342/files?mode=diff#note-8",
        ] {
            assert_eq!(chrome_match_prefix(url).unwrap(), expected);
        }
    }

    #[test]
    fn chrome_script_matches_only_at_url_boundaries() {
        let spec = chrome_command(712, "https://example.com/review").unwrap();
        assert!(spec.script.contains("matchesNavigationTarget"));
        assert!(spec.script.contains("matchPrefix & \"/\""));
        assert!(spec.script.contains("matchPrefix & \"#\""));
        assert!(spec.script.contains("matchPrefix & \"?\""));
    }

    #[test]
    fn chrome_script_reads_tab_urls_once_per_window() {
        let spec = chrome_command(712, "https://example.com").unwrap();
        assert!(spec.script.contains("valueForKey:\"URL\""));
        assert!(!spec.script.contains("URL of tab"));
    }

    #[test]
    fn chrome_script_activates_the_selected_pid_with_appkit() {
        let spec = chrome_command(712, "https://example.com").unwrap();
        assert!(spec.script.contains("runningApplicationWithProcessIdentifier"));
        assert!(spec.script.contains("activateWithOptions:3"));
        assert!(!spec.script.contains("chromeApp's |activate|()"));
    }

    #[test]
    fn chrome_script_reports_missing_target_without_constructing_tabs() {
        let spec = chrome_command(712, "https://example.com").unwrap();
        assert!(spec.script.contains("return \"NOT_FOUND\""));
        assert!(!spec.script.contains("classForScriptingClass"));
        assert!(!spec.script.contains("initWithProperties"));
    }

    #[test]
    fn chrome_open_fallback_uses_url_as_an_argument() {
        let hostile = "https://example.com/\"%20&%20do%20shell%20script%20\"bad\"";
        assert_eq!(chrome_open_args(hostile).unwrap(), vec!["--new-tab", hostile]);
    }

    #[test]
    fn chrome_script_compiles() {
        let stem = format!(
            "focus-bar-chrome-script-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let source_path = std::env::temp_dir().join(format!("{stem}.applescript"));
        let output_path = std::env::temp_dir().join(format!("{stem}.scpt"));
        std::fs::write(&source_path, CHROME_SCRIPT).unwrap();
        let output = std::process::Command::new("/usr/bin/osacompile")
            .arg("-l")
            .arg("AppleScript")
            .arg("-o")
            .arg(&output_path)
            .arg(&source_path)
            .output()
            .unwrap();
        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(output_path);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn vscode_window_lookup_uses_workspace_title_boundaries() {
        assert!(
            !VSCODE_SCRIPT.contains("name of w contains targetName"),
            "substring matching lets ling-design select ling-design-B"
        );
    }

    #[test]
    fn vscode_lookup_failure_uses_cli_fallback() {
        assert!(!vscode_window_was_found(false, ""));
    }

    #[test]
    fn vscode_cli_prefers_the_named_workspace_file_over_its_directory() {
        let root = std::env::temp_dir().join(format!(
            "focus-bar-vscode-workspace-{}",
            std::process::id()
        ));
        let workspace_file = root.join("ling-design.code-workspace");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&workspace_file, "{}").unwrap();

        assert_eq!(vscode_open_target(&root, "ling-design"), workspace_file);

        let _ = std::fs::remove_dir_all(root);
    }

    fn run_vscode_title_match(candidate: &str, target: &str) -> String {
        let output = std::process::Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(VSCODE_SCRIPT)
            .arg("--")
            .args(["__FOCUS_BAR_MATCH_TITLE__", candidate, target])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[test]
    fn vscode_workspace_title_match_does_not_confuse_name_prefixes() {
        assert_eq!(
            run_vscode_title_match(
                "page-pool.ts — ling-design-B (Workspace)",
                "ling-design"
            ),
            "NO_MATCH"
        );
        assert_eq!(
            run_vscode_title_match(
                "2026-07-13-img.md — ling-design (Workspace)",
                "ling-design"
            ),
            "MATCH"
        );
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
