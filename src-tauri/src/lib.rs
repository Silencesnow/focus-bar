use std::time::Duration;
use tauri::Manager;
use tokio::process::Command;
use tokio::time::timeout;

mod cmux_runtime;
pub mod activity_tracker;
mod activity_store;
#[cfg(target_os = "macos")]
mod activity_runtime;
pub mod codex_runtime;
pub mod task_timing;
pub mod task_timing_store;
mod task_timing_runtime;
#[cfg(target_os = "macos")]
mod cursor_watcher;
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

#[tauri::command]
fn open_activity_stats_window(app: tauri::AppHandle) -> Result<(), String> {
    let window = if let Some(window) = app.get_webview_window("stats") {
        window
    } else {
        tauri::WebviewWindowBuilder::new(
            &app,
            "stats",
            tauri::WebviewUrl::App("stats.html".into()),
        )
        .title("Focus Bar 活动统计")
        .inner_size(760.0, 650.0)
        .min_inner_size(620.0, 500.0)
        .center()
        .resizable(true)
        .build()
        .map_err(|error| format!("无法创建统计窗口: {error}"))?
    };
    window.show().map_err(|error| format!("无法显示统计窗口: {error}"))?;
    window.set_focus().map_err(|error| format!("无法聚焦统计窗口: {error}"))
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
            codex_runtime::fetch_codex_snapshot,
            navigation::focus_chrome_url,
            navigation::focus_vscode_target,
            #[cfg(target_os = "macos")]
            activity_runtime::note_codex_thread_opened,
            #[cfg(target_os = "macos")]
            activity_runtime::fetch_activity_summary,
            task_timing_runtime::record_task_status_snapshot,
            task_timing_runtime::fetch_task_timing_summary,
            task_config::save_task_navigation,
            read_home_file,
            write_home_file,
            home_dir,
            open_activity_stats_window,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            cursor_watcher::start(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
