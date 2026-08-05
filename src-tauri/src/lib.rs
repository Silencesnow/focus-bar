use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

mod cmux_runtime;
#[cfg(target_os = "macos")]
mod macos_hover;
mod navigation;
mod task_config;

#[tauri::command]
async fn shell_output(cmd: String, args: Vec<String>) -> Result<String, String> {
    let result = timeout(Duration::from_secs(5), async {
        Command::new(&cmd).args(&args).output().await
    }).await;
    match result {
        Ok(Ok(output)) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            } else {
                Err(String::from_utf8_lossy(&output.stderr).to_string())
            }
        }
        Ok(Err(e)) => Err(format!("Failed to execute {}: {}", cmd, e)),
        Err(_) => Err(format!("{} timed out after 5s", cmd)),
    }
}

#[tauri::command]
fn read_home_file(path: String) -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let full = home.join(&path);
    if !full.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&full).map_err(|e| format!("Failed to read {}: {}", full.display(), e))
}

#[tauri::command]
fn write_home_file(path: String, content: String) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let full = home.join(&path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create dirs: {}", e))?;
    }
    std::fs::write(&full, content).map_err(|e| format!("Failed to write {}: {}", full.display(), e))
}

#[tauri::command]
fn home_dir() -> Result<String, String> {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or("Cannot find home directory".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            shell_output,
            cmux_runtime::fetch_cmux_snapshot,
            cmux_runtime::start_cmux_watcher,
            cmux_runtime::focus_cmux_workspace,
            navigation::focus_chrome_url,
            navigation::focus_vscode_target,
            task_config::save_task_navigation,
            read_home_file,
            write_home_file,
            home_dir,
        ])
        .on_page_load(|webview, payload| {
            #[cfg(target_os = "macos")]
            if webview.label() == "main"
                && matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)
            {
                if let Err(error) = macos_hover::enable_inactive_hover(webview) {
                    eprintln!("failed to enable inactive hover: {error}");
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
