use focus_bar_lib::task_timing::{
    aggregate_task_timing, update_task_state, CompletedInterval, TaskStatus, TaskStatusSample,
    TaskTimingKind, TaskTimingState,
};

fn sample(task_id: &str, status: TaskStatus) -> TaskStatusSample {
    TaskStatusSample {
        task_id: task_id.into(),
        task_title: format!("Task {task_id}"),
        source: "cmux".into(),
        status,
    }
}

fn state(task_id: &str, status: TaskStatus, started_at: u64, last_seen_at: u64) -> TaskTimingState {
    TaskTimingState {
        task_id: task_id.into(),
        task_title: format!("Task {task_id}"),
        source: "cmux".into(),
        status,
        status_started_at: started_at,
        last_seen_at,
        pending_started_at: None,
    }
}

#[test]
fn leaving_execution_closes_an_execution_interval() {
    let previous = state("a", TaskStatus::Executing, 1_000, 4_000);

    let update = update_task_state(Some(previous), &sample("a", TaskStatus::NeedsReview), 5_000, 15_000);

    assert_eq!(update.completed.len(), 1);
    assert_eq!(update.completed[0].kind, TaskTimingKind::Execution);
    assert_eq!((update.completed[0].started_at, update.completed[0].ended_at), (1_000, 5_000));
    assert_eq!(update.state.pending_started_at, Some(5_000));
}

#[test]
fn red_through_idle_closes_only_when_the_task_executes_again() {
    let mut pending = state("a", TaskStatus::NeedsAction, 1_000, 1_000);
    pending.pending_started_at = Some(1_000);

    let idle = update_task_state(Some(pending), &sample("a", TaskStatus::Idle), 2_000, 15_000);
    assert!(idle.completed.is_empty());
    assert_eq!(idle.state.pending_started_at, Some(1_000));

    let executing = update_task_state(Some(idle.state), &sample("a", TaskStatus::Executing), 5_000, 15_000);
    assert_eq!(executing.completed.len(), 1);
    assert_eq!(executing.completed[0].kind, TaskTimingKind::Interruption);
    assert_eq!((executing.completed[0].started_at, executing.completed[0].ended_at), (1_000, 5_000));
    assert_eq!(executing.state.pending_started_at, None);
}

#[test]
fn an_unclosed_red_tail_produces_no_interval() {
    let update = update_task_state(None, &sample("a", TaskStatus::NeedsReview), 1_000, 15_000);

    assert!(update.completed.is_empty());
    assert_eq!(update.state.pending_started_at, Some(1_000));
}

#[test]
fn a_sampling_gap_does_not_bridge_execution_or_interruption() {
    let mut previous = state("a", TaskStatus::NeedsReview, 1_000, 2_000);
    previous.pending_started_at = Some(1_000);

    let update = update_task_state(Some(previous), &sample("a", TaskStatus::Executing), 20_000, 15_000);

    assert!(update.completed.is_empty());
    assert_eq!(update.state.status_started_at, 20_000);
    assert_eq!(update.state.pending_started_at, None);
}

fn interval(task_id: &str, kind: TaskTimingKind, started_at: u64, ended_at: u64) -> CompletedInterval {
    CompletedInterval {
        task_id: task_id.into(),
        task_title: format!("Task {task_id}"),
        source: "cmux".into(),
        kind,
        started_at,
        ended_at,
    }
}

#[test]
fn concurrent_execution_is_summed_per_task_and_merged_for_wall_clock_coverage() {
    let summary = aggregate_task_timing(&[
        interval("a", TaskTimingKind::Execution, 0, 10_000),
        interval("b", TaskTimingKind::Execution, 5_000, 15_000),
    ], 0, 20_000);

    assert_eq!(summary.task_execution_ms, 20_000);
    assert_eq!(summary.actual_execution_ms, 15_000);
    assert_eq!(summary.tasks.len(), 2);
}

#[test]
fn aggregation_clips_execution_and_closed_interruptions_to_the_range() {
    let summary = aggregate_task_timing(&[
        interval("a", TaskTimingKind::Execution, 5_000, 15_000),
        interval("a", TaskTimingKind::Interruption, 6_000, 18_000),
    ], 10_000, 20_000);

    assert_eq!(summary.task_execution_ms, 5_000);
    assert_eq!(summary.actual_execution_ms, 5_000);
    assert_eq!(summary.tasks[0].execution_ms, 5_000);
    assert_eq!(summary.tasks[0].interruption_ms, 8_000);
    assert_eq!(summary.tasks[0].execution_count, 1);
    assert_eq!(summary.tasks[0].interruption_count, 1);
}

#[test]
fn segments_preserve_the_execution_interruption_execution_rhythm() {
    let summary = aggregate_task_timing(&[
        interval("a", TaskTimingKind::Execution, 0, 4_000),
        interval("a", TaskTimingKind::Interruption, 4_000, 6_000),
        interval("a", TaskTimingKind::Execution, 6_000, 9_000),
        interval("a", TaskTimingKind::Interruption, 9_000, 10_000),
    ], 0, 20_000);

    let segments = &summary.tasks[0].segments;
    assert_eq!(segments.len(), 4);
    assert_eq!((segments[0].kind, segments[0].duration_ms), (TaskTimingKind::Execution, 4_000));
    assert_eq!((segments[1].kind, segments[1].duration_ms), (TaskTimingKind::Interruption, 2_000));
    assert_eq!((segments[2].kind, segments[2].duration_ms), (TaskTimingKind::Execution, 3_000));
    assert_eq!((segments[3].kind, segments[3].duration_ms), (TaskTimingKind::Interruption, 1_000));
}

#[test]
fn segments_are_clipped_to_the_range() {
    let summary = aggregate_task_timing(&[
        interval("a", TaskTimingKind::Execution, 0, 8_000),
        interval("a", TaskTimingKind::Interruption, 8_000, 12_000),
    ], 5_000, 10_000);

    let segments = &summary.tasks[0].segments;
    assert_eq!(segments.len(), 2);
    assert_eq!((segments[0].kind, segments[0].duration_ms), (TaskTimingKind::Execution, 3_000));
    assert_eq!((segments[1].kind, segments[1].duration_ms), (TaskTimingKind::Interruption, 2_000));
}
