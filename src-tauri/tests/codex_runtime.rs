use focus_bar_lib::codex_runtime::{parse_transcript, CodexLifecycle};

#[test]
fn unmatched_task_start_remains_executing_even_when_the_last_event_is_old() {
    let transcript = r#"
{"timestamp":"2026-07-16T08:00:00Z","type":"event_msg","payload":{"type":"task_started"}}
{"timestamp":"2026-07-16T08:01:00Z","type":"event_msg","payload":{"type":"agent_message","message":"后台命令仍在运行"}}
"#;

    let state = parse_transcript(transcript);

    assert_eq!(state.lifecycle, CodexLifecycle::Executing);
    assert_eq!(state.activity_at.as_deref(), Some("2026-07-16T08:01:00Z"));
    assert_eq!(state.latest_message.as_deref(), Some("后台命令仍在运行"));
}

#[test]
fn completed_task_uses_the_completion_message_and_timestamp() {
    let transcript = r#"
{"timestamp":"2026-07-16T08:00:00Z","type":"event_msg","payload":{"type":"task_started"}}
{"timestamp":"2026-07-16T08:05:00Z","type":"event_msg","payload":{"type":"task_complete","last_agent_message":"改动已完成"}}
"#;

    let state = parse_transcript(transcript);

    assert_eq!(state.lifecycle, CodexLifecycle::Completed);
    assert_eq!(state.activity_at.as_deref(), Some("2026-07-16T08:05:00Z"));
    assert_eq!(state.latest_message.as_deref(), Some("改动已完成"));
}

#[test]
fn outstanding_request_user_input_is_actionable() {
    let transcript = r#"
{"timestamp":"2026-07-16T08:00:00Z","type":"event_msg","payload":{"type":"task_started"}}
{"timestamp":"2026-07-16T08:02:00Z","type":"response_item","payload":{"type":"custom_tool_call","name":"request_user_input","call_id":"call-1"}}
"#;

    let state = parse_transcript(transcript);

    assert_eq!(state.lifecycle, CodexLifecycle::NeedsInput);
}

#[test]
fn answered_request_user_input_returns_to_executing() {
    let transcript = r#"
{"timestamp":"2026-07-16T08:00:00Z","type":"event_msg","payload":{"type":"task_started"}}
{"timestamp":"2026-07-16T08:02:00Z","type":"response_item","payload":{"type":"custom_tool_call","name":"request_user_input","call_id":"call-1"}}
{"timestamp":"2026-07-16T08:03:00Z","type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"call-1","output":"choice"}}
"#;

    let state = parse_transcript(transcript);

    assert_eq!(state.lifecycle, CodexLifecycle::Executing);
}

#[test]
#[ignore = "reads the current user's local Codex state"]
fn fetches_the_local_codex_snapshot() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let snapshot = runtime.block_on(focus_bar_lib::codex_runtime::fetch_codex_snapshot());
    assert!(!snapshot.threads.is_empty());
    assert!(snapshot
        .threads
        .iter()
        .any(|thread| { thread.lifecycle == CodexLifecycle::Executing }));
    for thread in snapshot.threads.iter().take(5) {
        eprintln!("{} {:?} {}", thread.id, thread.lifecycle, thread.title);
    }
}
