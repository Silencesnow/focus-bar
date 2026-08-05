use crate::activity_tracker::{ActivitySegment, ActivitySource, ActivityType, Confidence};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone)]
pub struct ActivityStore {
    path: PathBuf,
}

impl ActivityStore {
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
             CREATE TABLE IF NOT EXISTS activity_segments(\
               id INTEGER PRIMARY KEY,\
               started_at INTEGER NOT NULL,\
               ended_at INTEGER NOT NULL,\
               source TEXT NOT NULL,\
               activity_type TEXT NOT NULL,\
               task_id TEXT,\
               task_title TEXT,\
               confidence TEXT NOT NULL,\
               context_key TEXT\
             );\
             CREATE INDEX IF NOT EXISTS idx_activity_segments_range \
               ON activity_segments(started_at, ended_at);",
        )?;
        Ok(())
    }

    pub fn insert(&self, segment: &ActivitySegment) -> Result<(), String> {
        let sql = format!(
            "INSERT INTO activity_segments(\
               started_at, ended_at, source, activity_type, task_id, task_title, confidence, context_key\
             ) VALUES ({}, {}, {}, {}, {}, {}, {}, {});",
            segment.started_at,
            segment.ended_at,
            sql_text(enum_name(segment.source)),
            sql_text(enum_name(segment.activity_type)),
            sql_optional(segment.task_id.as_deref()),
            sql_optional(segment.task_title.as_deref()),
            sql_text(enum_name(segment.confidence)),
            sql_optional(segment.context_key.as_deref()),
        );
        self.run_sql(&sql)?;
        Ok(())
    }

    pub fn load_segments(&self, range_start: u64, range_end: u64) -> Result<Vec<ActivitySegment>, String> {
        let sql = format!(
            "SELECT started_at, ended_at, source, activity_type, task_id, task_title, confidence, context_key \
             FROM activity_segments WHERE ended_at > {range_start} AND started_at < {range_end} \
             ORDER BY started_at;"
        );
        let raw = self.run_sql(&sql)?;
        let rows: Vec<Value> = serde_json::from_str(if raw.trim().is_empty() { "[]" } else { &raw })
            .map_err(|error| format!("无法解析活动记录: {error}"))?;
        rows.into_iter().map(parse_segment).collect()
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

fn enum_name<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn sql_text(value: String) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sql_optional(value: Option<&str>) -> String {
    value.map(|value| sql_text(value.to_string())).unwrap_or_else(|| "NULL".into())
}

fn parse_segment(row: Value) -> Result<ActivitySegment, String> {
    let string = |key: &str| row.get(key).and_then(Value::as_str).map(str::to_string);
    Ok(ActivitySegment {
        started_at: row.get("started_at").and_then(Value::as_u64).ok_or("活动记录缺少 started_at")?,
        ended_at: row.get("ended_at").and_then(Value::as_u64).ok_or("活动记录缺少 ended_at")?,
        source: parse_enum::<ActivitySource>(&row, "source")?,
        activity_type: parse_enum::<ActivityType>(&row, "activity_type")?,
        task_id: string("task_id"),
        task_title: string("task_title"),
        confidence: parse_enum::<Confidence>(&row, "confidence")?,
        context_key: string("context_key"),
    })
}

fn parse_enum<T: serde::de::DeserializeOwned>(row: &Value, key: &str) -> Result<T, String> {
    let value = row
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("活动记录缺少 {key}"))?;
    serde_json::from_value(Value::String(value.to_string())).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("focus-bar-activity-{}.sqlite3", std::process::id()))
    }

    #[test]
    fn persists_and_reads_segments_in_a_time_range() {
        let path = temp_db();
        let _ = std::fs::remove_file(&path);
        let store = ActivityStore::new(path.clone());
        store.initialize().unwrap();
        store.insert(&ActivitySegment {
            started_at: 1_000,
            ended_at: 5_000,
            source: ActivitySource::Cmux,
            activity_type: ActivityType::AiInput,
            task_id: Some("task-a".into()),
            task_title: Some("Task A".into()),
            confidence: Confidence::High,
            context_key: Some("workspace-a".into()),
        }).unwrap();

        let loaded = store.load_segments(0, 10_000).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].task_id.as_deref(), Some("task-a"));
        assert_eq!(loaded[0].activity_type, ActivityType::AiInput);
        let _ = std::fs::remove_file(path);
    }
}
