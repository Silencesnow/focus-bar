use serde_json::{Map, Value};
use std::path::PathBuf;

pub fn config_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "找不到用户主目录".to_string())?;
    Ok(home.join(".config").join("cmux").join("cmux.json"))
}

pub fn ensure_allow_all(existing: Option<&str>) -> Result<(String, bool), String> {
    let mut root: Value = match existing {
        Some(raw) if !raw.trim().is_empty() => {
            serde_json::from_str(raw).map_err(|error| format!("现有 cmux.json 无法解析: {error}"))?
        }
        _ => Value::Object(Map::new()),
    };
    if !root.is_object() {
        return Err("现有 cmux.json 顶层不是对象".to_string());
    }

    let obj = root.as_object_mut().unwrap();
    obj.entry("$schema").or_insert_with(|| {
        Value::String(
            "https://raw.githubusercontent.com/manaflow-ai/cmux/main/web/data/cmux.schema.json"
                .to_string(),
        )
    });
    obj.entry("schemaVersion").or_insert_with(|| Value::from(1));

    let automation = obj
        .entry("automation")
        .or_insert_with(|| Value::Object(Map::new()));
    if !automation.is_object() {
        return Err("现有 cmux.json 的 automation 字段不是对象".to_string());
    }
    let automation = automation.as_object_mut().unwrap();

    let already = automation
        .get("socketControlMode")
        .and_then(Value::as_str)
        .map(|mode| mode == "allowAll")
        .unwrap_or(false);
    if already {
        let serialized = serde_json::to_string_pretty(&root)
            .map_err(|error| format!("序列化 cmux.json 失败: {error}"))?;
        return Ok((serialized, false));
    }

    automation.insert(
        "socketControlMode".to_string(),
        Value::String("allowAll".to_string()),
    );
    let serialized = serde_json::to_string_pretty(&root)
        .map_err(|error| format!("序列化 cmux.json 失败: {error}"))?;
    Ok((serialized, true))
}
