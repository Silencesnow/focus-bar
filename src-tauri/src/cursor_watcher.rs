use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::time::{interval, MissedTickBehavior};

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
struct CursorPoint {
    x: f64,
    y: f64,
}

fn should_emit_cursor(previous: Option<CursorPoint>, current: CursorPoint) -> bool {
    previous != Some(current)
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = interval(Duration::from_millis(50));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut previous = None;

        loop {
            ticker.tick().await;
            let Ok(position) = app.cursor_position() else {
                continue;
            };
            let current = CursorPoint {
                x: position.x,
                y: position.y,
            };
            if should_emit_cursor(previous, current) {
                previous = Some(current);
                let _ = app.emit_to("main", "global-cursor-moved", current);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_only_when_the_global_cursor_position_changes() {
        let current = CursorPoint { x: 120.0, y: 40.0 };

        assert!(should_emit_cursor(None, current));
        assert!(!should_emit_cursor(Some(current), current));
        assert!(should_emit_cursor(
            Some(current),
            CursorPoint { x: 121.0, y: 40.0 },
        ));
    }
}
