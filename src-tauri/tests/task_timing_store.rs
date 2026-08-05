use focus_bar_lib::task_timing::{TaskStatus, TaskStatusSample, TaskTimingKind};
use focus_bar_lib::task_timing_store::TaskTimingStore;

fn path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("focus-bar-{name}-{}.sqlite3", std::process::id()))
}

fn sample(status: TaskStatus) -> TaskStatusSample {
    TaskStatusSample {
        task_id: "task-a".into(),
        task_title: "Task A".into(),
        source: "cmux".into(),
        status,
    }
}

#[test]
fn same_status_heartbeats_extend_execution_until_the_status_changes() {
    let path = path("timing-heartbeat");
    let _ = std::fs::remove_file(&path);
    let store = TaskTimingStore::new(path.clone());
    store.initialize().unwrap();

    store.record_snapshot(&[sample(TaskStatus::Executing)], 1_000, 15_000).unwrap();
    store.record_snapshot(&[sample(TaskStatus::Executing)], 5_000, 15_000).unwrap();
    store.record_snapshot(&[sample(TaskStatus::Idle)], 8_000, 15_000).unwrap();

    let intervals = store.load_intervals(0, 20_000).unwrap();
    assert_eq!(intervals.len(), 1);
    assert_eq!(intervals[0].kind, TaskTimingKind::Execution);
    assert_eq!((intervals[0].started_at, intervals[0].ended_at), (1_000, 8_000));
    let _ = std::fs::remove_file(path);
}

#[test]
fn red_idle_green_persists_one_closed_interruption() {
    let path = path("timing-interruption");
    let _ = std::fs::remove_file(&path);
    let store = TaskTimingStore::new(path.clone());
    store.initialize().unwrap();

    store.record_snapshot(&[sample(TaskStatus::NeedsReview)], 10_000, 15_000).unwrap();
    store.record_snapshot(&[sample(TaskStatus::Idle)], 12_000, 15_000).unwrap();
    store.record_snapshot(&[sample(TaskStatus::Executing)], 20_000, 15_000).unwrap();

    let intervals = store.load_intervals(0, 30_000).unwrap();
    assert_eq!(intervals.len(), 1);
    assert_eq!(intervals[0].kind, TaskTimingKind::Interruption);
    assert_eq!((intervals[0].started_at, intervals[0].ended_at), (10_000, 20_000));
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_gap_closes_known_execution_at_last_seen_without_bridging_the_gap() {
    let path = path("timing-gap");
    let _ = std::fs::remove_file(&path);
    let store = TaskTimingStore::new(path.clone());
    store.initialize().unwrap();

    store.record_snapshot(&[sample(TaskStatus::Executing)], 1_000, 15_000).unwrap();
    store.record_snapshot(&[sample(TaskStatus::Executing)], 2_000, 15_000).unwrap();
    store.record_snapshot(&[sample(TaskStatus::Idle)], 20_000, 15_000).unwrap();

    let intervals = store.load_intervals(0, 30_000).unwrap();
    assert_eq!(intervals.len(), 1);
    assert_eq!((intervals[0].started_at, intervals[0].ended_at), (1_000, 2_000));
    let _ = std::fs::remove_file(path);
}

#[test]
fn an_open_execution_is_reported_only_through_its_latest_heartbeat() {
    let path = path("timing-open");
    let _ = std::fs::remove_file(&path);
    let store = TaskTimingStore::new(path.clone());
    store.initialize().unwrap();

    store.record_snapshot(&[sample(TaskStatus::Executing)], 1_000, 15_000).unwrap();
    store.record_snapshot(&[sample(TaskStatus::Executing)], 5_000, 15_000).unwrap();

    let intervals = store.load_intervals(0, 30_000).unwrap();
    assert_eq!(intervals.len(), 1);
    assert_eq!((intervals[0].started_at, intervals[0].ended_at), (1_000, 5_000));
    let _ = std::fs::remove_file(path);
}
