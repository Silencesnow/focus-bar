use focus_bar_lib::cmux_config::ensure_allow_all;
use serde_json::Value;

fn parse(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap()
}

#[test]
fn creates_a_full_config_when_none_exists() {
    let (serialized, changed) = ensure_allow_all(None).unwrap();
    assert!(changed);
    let value = parse(&serialized);
    assert_eq!(value["automation"]["socketControlMode"], "allowAll");
    assert_eq!(value["schemaVersion"], 1);
    assert!(value["$schema"].as_str().unwrap().contains("cmux.schema.json"));
}

#[test]
fn adds_allow_all_while_preserving_other_fields() {
    let existing = r#"{
        "schemaVersion": 1,
        "automation": { "somethingElse": true },
        "theme": "dark"
    }"#;
    let (serialized, changed) = ensure_allow_all(Some(existing)).unwrap();
    assert!(changed);
    let value = parse(&serialized);
    assert_eq!(value["automation"]["socketControlMode"], "allowAll");
    assert_eq!(value["automation"]["somethingElse"], true);
    assert_eq!(value["theme"], "dark");
}

#[test]
fn reports_no_change_when_already_allow_all() {
    let existing = r#"{ "automation": { "socketControlMode": "allowAll" } }"#;
    let (_, changed) = ensure_allow_all(Some(existing)).unwrap();
    assert!(!changed);
}

#[test]
fn overwrites_a_different_socket_control_mode() {
    let existing = r#"{ "automation": { "socketControlMode": "cmuxOnly" } }"#;
    let (serialized, changed) = ensure_allow_all(Some(existing)).unwrap();
    assert!(changed);
    assert_eq!(parse(&serialized)["automation"]["socketControlMode"], "allowAll");
}

#[test]
fn rejects_malformed_json() {
    assert!(ensure_allow_all(Some("{ not json")).is_err());
}
