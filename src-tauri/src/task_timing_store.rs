use crate::task_timing::{
    update_task_state, CompletedInterval, TaskStatus, TaskStatusSample, TaskTimingKind,
    TaskTimingState,
};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone)]
pub struct TaskTimingStore {
    path: PathBuf,
}

impl TaskTimingStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn default_path() -> Result<PathBuf, String> {
        let root = dirs::data_local_dir().ok_or_else(|| "找不到本地应用数据目录".to_string())?;
        Ok(root.join("com.shamingming.focus-bar").join("activity.sqlite3"))
    }

    pub fn initialize(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        self.run_sql(
            "PRAGMA journal_mode=WAL;\
             CREATE TABLE IF NOT EXISTS task_timing_state(\
               task_id TEXT PRIMARY KEY,\
               task_title TEXT NOT NULL,\
               source TEXT NOT NULL,\
               status TEXT NOT NULL,\
               status_started_at INTEGER NOT NULL,\
               last_seen_at INTEGER NOT NULL,\
               pending_started_at INTEGER\
             );\
             CREATE TABLE IF NOT EXISTS task_timing_intervals(\
               id INTEGER PRIMARY KEY,\
               task_id TEXT NOT NULL,\
               task_title TEXT NOT NULL,\
               source TEXT NOT NULL,\
               kind TEXT NOT NULL,\
               started_at INTEGER NOT NULL,\
               ended_at INTEGER NOT NULL\
             );\
             CREATE INDEX IF NOT EXISTS idx_task_timing_intervals_range \
               ON task_timing_intervals(started_at, ended_at);",
        )?;
        Ok(())
    }

    pub fn record_snapshot(
        &self,
        samples: &[TaskStatusSample],
        observed_at: u64,
        max_gap_ms: u64,
    ) -> Result<(), String> {
        if samples.is_empty() {
            return Ok(());
        }
        let mut states = self.load_states()?;
        let mut sql = String::from("BEGIN IMMEDIATE;");
        for sample in samples {
            let update = update_task_state(
                states.remove(&sample.task_id),
                sample,
                observed_at,
                max_gap_ms,
            );
            for interval in &update.completed {
                sql.push_str(&insert_interval_sql(interval));
            }
            sql.push_str(&upsert_state_sql(&update.state));
            states.insert(sample.task_id.clone(), update.state);
        }
        sql.push_str("COMMIT;");
        self.run_sql(&sql)?;
        Ok(())
    }

    pub fn load_intervals(
        &self,
        range_start: u64,
        range_end: u64,
    ) -> Result<Vec<CompletedInterval>, String> {
        let sql = format!(
            "SELECT task_id, task_title, source, kind, started_at, ended_at \
             FROM task_timing_intervals \
             WHERE ended_at > {range_start} AND started_at < {range_end} \
             ORDER BY started_at;"
        );
        let raw = self.run_sql(&sql)?;
        let rows: Vec<Value> = parse_rows(&raw)?;
        let mut intervals = rows.into_iter().map(parse_interval).collect::<Result<Vec<_>, _>>()?;
        for state in self.load_states()?.into_values() {
            if state.status == TaskStatus::Executing && state.last_seen_at > state.status_started_at {
                intervals.push(CompletedInterval {
                    task_id: state.task_id,
                    task_title: state.task_title,
                    source: state.source,
                    kind: TaskTimingKind::Execution,
                    started_at: state.status_started_at,
                    ended_at: state.last_seen_at,
                });
            }
        }
        Ok(intervals)
    }

    fn load_states(&self) -> Result<HashMap<String, TaskTimingState>, String> {
        let raw = self.run_sql(
            "SELECT task_id, task_title, source, status, status_started_at, last_seen_at, pending_started_at \
             FROM task_timing_state;",
        )?;
        parse_rows(&raw)?
            .into_iter()
            .map(parse_state)
            .map(|result| result.map(|state| (state.task_id.clone(), state)))
            .collect()
    }

    fn run_sql(&self, sql: &str) -> Result<String, String> {
        let output = Command::new("/usr/bin/sqlite3")
            .args(["-json", "-cmd", ".timeout 1000", self.path.to_string_lossy().as_ref(), sql])
            .output()
            .map_err(|error| format!("无法启动 sqlite3: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

fn parse_rows(raw: &str) -> Result<Vec<Value>, String> {
    serde_json::from_str(if raw.trim().is_empty() { "[]" } else { raw })
        .map_err(|error| format!("无法解析任务时间记录: {error}"))
}

fn enum_name<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn sql_text(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sql_optional_u64(value: Option<u64>) -> String {
    value.map(|value| value.to_string()).unwrap_or_else(|| "NULL".into())
}

fn upsert_state_sql(state: &TaskTimingState) -> String {
    format!(
        "INSERT INTO task_timing_state(\
           task_id, task_title, source, status, status_started_at, last_seen_at, pending_started_at\
         ) VALUES ({}, {}, {}, {}, {}, {}, {}) \
         ON CONFLICT(task_id) DO UPDATE SET \
           task_title=excluded.task_title, source=excluded.source, status=excluded.status, \
           status_started_at=excluded.status_started_at, last_seen_at=excluded.last_seen_at, \
           pending_started_at=excluded.pending_started_at;",
        sql_text(&state.task_id),
        sql_text(&state.task_title),
        sql_text(&state.source),
        sql_text(&enum_name(state.status)),
        state.status_started_at,
        state.last_seen_at,
        sql_optional_u64(state.pending_started_at),
    )
}

fn insert_interval_sql(interval: &CompletedInterval) -> String {
    format!(
        "INSERT INTO task_timing_intervals(\
           task_id, task_title, source, kind, started_at, ended_at\
         ) VALUES ({}, {}, {}, {}, {}, {});",
        sql_text(&interval.task_id),
        sql_text(&interval.task_title),
        sql_text(&interval.source),
        sql_text(&enum_name(interval.kind)),
        interval.started_at,
        interval.ended_at,
    )
}

fn row_string(row: &Value, key: &str) -> Result<String, String> {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("任务时间记录缺少 {key}"))
}

fn row_u64(row: &Value, key: &str) -> Result<u64, String> {
    row.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("任务时间记录缺少 {key}"))
}

fn parse_status(value: String) -> Result<TaskStatus, String> {
    serde_json::from_value(Value::String(value)).map_err(|error| error.to_string())
}

fn parse_kind(value: String) -> Result<TaskTimingKind, String> {
    serde_json::from_value(Value::String(value)).map_err(|error| error.to_string())
}

fn parse_state(row: Value) -> Result<TaskTimingState, String> {
    Ok(TaskTimingState {
        task_id: row_string(&row, "task_id")?,
        task_title: row_string(&row, "task_title")?,
        source: row_string(&row, "source")?,
        status: parse_status(row_string(&row, "status")?)?,
        status_started_at: row_u64(&row, "status_started_at")?,
        last_seen_at: row_u64(&row, "last_seen_at")?,
        pending_started_at: row.get("pending_started_at").and_then(Value::as_u64),
    })
}

fn parse_interval(row: Value) -> Result<CompletedInterval, String> {
    Ok(CompletedInterval {
        task_id: row_string(&row, "task_id")?,
        task_title: row_string(&row, "task_title")?,
        source: row_string(&row, "source")?,
        kind: parse_kind(row_string(&row, "kind")?)?,
        started_at: row_u64(&row, "started_at")?,
        ended_at: row_u64(&row, "ended_at")?,
    })
}
