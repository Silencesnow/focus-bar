use crate::navigation::{
    validate_chrome_url, vscode_goto_target, NavigationError, NavigationErrorCode,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromeInput {
    pub label: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VscodeInput {
    pub workspace: String,
    pub workspace_name: String,
    pub file: Option<String>,
    pub line: Option<u32>,
}

fn config_error(message: impl Into<String>, detail: Option<String>) -> NavigationError {
    NavigationError::new(NavigationErrorCode::TargetCommandFailed, message, detail)
}

fn validate_inputs(
    chrome: &Option<Vec<ChromeInput>>,
    vscode: &Option<VscodeInput>,
) -> Result<(), NavigationError> {
    if let Some(chrome) = chrome {
        for target in chrome {
            validate_chrome_url(target.url.trim())?;
        }
    }
    if let Some(vscode) = vscode {
        vscode_goto_target(
            Path::new(vscode.workspace.trim()),
            vscode.file.as_deref(),
            vscode.line,
        )?;
    }
    Ok(())
}

fn merge_task_navigation(
    data: &mut Value,
    task_id: &str,
    name: &str,
    name_overridden: bool,
    icon: &str,
    chrome: Option<Vec<ChromeInput>>,
    vscode: Option<VscodeInput>,
) -> Result<Value, NavigationError> {
    validate_inputs(&chrome, &vscode)?;
    let tasks = data
        .get_mut("tasks")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            NavigationError::new(
                NavigationErrorCode::InvalidTarget,
                "Focus data does not contain a tasks array",
                None,
            )
        })?;
    let task = tasks
        .iter_mut()
        .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            NavigationError::new(
                NavigationErrorCode::InvalidTarget,
                format!("Task {task_id} does not exist"),
                None,
            )
        })?;

    task.insert("name".into(), Value::String(name.trim().to_string()));
    task.insert("name_overridden".into(), Value::Bool(name_overridden));
    let icon = icon.trim();
    if icon.is_empty() {
        task.remove("tab_icon");
    } else {
        task.insert("tab_icon".into(), Value::String(icon.to_string()));
    }
    match chrome {
        Some(mut chrome) if !chrome.is_empty() => {
            for target in &mut chrome {
                target.url = target.url.trim().to_string();
                target.label = target.label.take().and_then(|label| {
                    let trimmed = label.trim().to_string();
                    (!trimmed.is_empty()).then_some(trimmed)
                });
            }
            task.insert("chrome".into(), serde_json::to_value(chrome).unwrap());
        }
        Some(_) | None => {
            task.remove("chrome");
        }
    }
    match vscode {
        Some(mut vscode) => {
            vscode.workspace = vscode.workspace.trim().to_string();
            vscode.workspace_name = if vscode.workspace_name.trim().is_empty() {
                Path::new(&vscode.workspace)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                vscode.workspace_name.trim().to_string()
            };
            vscode.file = vscode.file.and_then(|value| {
                let trimmed = value.trim().to_string();
                (!trimmed.is_empty()).then_some(trimmed)
            });
            task.insert("vscode".into(), serde_json::to_value(vscode).unwrap());
        }
        None => {
            task.remove("vscode");
        }
    }
    Ok(Value::Object(task.clone()))
}

#[tauri::command]
pub fn save_task_navigation(
    app: AppHandle,
    task_id: String,
    name: String,
    name_overridden: bool,
    icon: String,
    chrome: Option<Vec<ChromeInput>>,
    vscode: Option<VscodeInput>,
) -> Result<Value, NavigationError> {
    let home = dirs::home_dir().ok_or_else(|| config_error("Cannot find home directory", None))?;
    let path = home.join(".focus.json");
    let raw = std::fs::read_to_string(&path)
        .map_err(|error| config_error("Could not read ~/.focus.json", Some(error.to_string())))?;
    let mut data: Value = serde_json::from_str(&raw)
        .map_err(|error| config_error("Could not parse ~/.focus.json", Some(error.to_string())))?;
    let updated = merge_task_navigation(
        &mut data,
        &task_id,
        &name,
        name_overridden,
        &icon,
        chrome,
        vscode,
    )?;
    let temp_path = path.with_extension("json.tmp");
    let content = serde_json::to_vec_pretty(&data)
        .map_err(|error| config_error("Could not encode focus data", Some(error.to_string())))?;
    let mut file = File::create(&temp_path).map_err(|error| {
        config_error(
            "Could not create temporary focus data",
            Some(error.to_string()),
        )
    })?;
    file.write_all(&content).map_err(|error| {
        config_error(
            "Could not write temporary focus data",
            Some(error.to_string()),
        )
    })?;
    file.sync_all().map_err(|error| {
        config_error(
            "Could not sync temporary focus data",
            Some(error.to_string()),
        )
    })?;
    std::fs::rename(&temp_path, &path)
        .map_err(|error| config_error("Could not replace focus data", Some(error.to_string())))?;
    let _ = app.emit(
        "focus-config-changed",
        serde_json::json!({ "task_id": task_id }),
    );
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_preserves_unrelated_task_fields() {
        let mut data = json!({"tasks": [{
            "id": "task-1", "name": "Old", "cmux_workspace_id": "workspace-1",
            "manual_status": "needs_action", "note": "keep me", "unknown": 7
        }]});
        let updated = merge_task_navigation(
            &mut data,
            "task-1",
            "New",
            true,
            "FE",
            Some(vec![
                ChromeInput {
                    label: Some(" Web MR ".into()),
                    url: " https://example.com/web ".into(),
                },
                ChromeInput {
                    label: Some("API MR".into()),
                    url: "https://example.com/api".into(),
                },
            ]),
            Some(VscodeInput {
                workspace: "/tmp/app".into(),
                workspace_name: "app".into(),
                file: Some("src/main.ts".into()),
                line: Some(12),
            }),
        )
        .unwrap();
        assert_eq!(updated["name"], "New");
        assert_eq!(updated["name_overridden"], true);
        assert_eq!(updated["tab_icon"], "FE");
        assert_eq!(updated["note"], "keep me");
        assert_eq!(updated["unknown"], 7);
        assert_eq!(updated["chrome"][0]["label"], "Web MR");
        assert_eq!(updated["chrome"][0]["url"], "https://example.com/web");
        assert_eq!(updated["chrome"][1]["label"], "API MR");
        assert_eq!(updated["vscode"]["workspace_name"], "app");
    }

    #[test]
    fn empty_targets_remove_existing_groups() {
        let mut data = json!({"tasks": [{
            "id": "task-1", "name": "Old",
            "chrome": {"url": "https://old.example"},
            "vscode": {"workspace": "/tmp/old"}
        }]});
        let updated =
            merge_task_navigation(&mut data, "task-1", "Old", false, "", None, None).unwrap();
        assert!(updated.get("chrome").is_none());
        assert!(updated.get("vscode").is_none());
    }

    #[test]
    fn local_file_chrome_target_can_be_saved() {
        let mut data = json!({"tasks": [{"id": "task-1", "name": "Task"}]});
        let url = "file:///Users/test/Documents/preview.html";

        let updated = merge_task_navigation(
            &mut data,
            "task-1",
            "Task",
            false,
            "",
            Some(vec![ChromeInput {
                label: Some("Html".into()),
                url: url.into(),
            }]),
            None,
        )
        .unwrap();

        assert_eq!(updated["chrome"][0]["url"], url);
    }

    #[test]
    fn missing_task_is_an_invalid_target() {
        let mut data = json!({"tasks": []});
        assert_eq!(
            merge_task_navigation(&mut data, "missing", "Name", false, "", None, None)
                .unwrap_err()
                .code,
            NavigationErrorCode::InvalidTarget
        );
    }
}
