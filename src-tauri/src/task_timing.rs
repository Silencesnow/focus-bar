use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    NeedsAction,
    NeedsReview,
    Executing,
    Idle,
}

impl TaskStatus {
    fn is_pending(self) -> bool {
        matches!(self, Self::NeedsAction | Self::NeedsReview)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStatusSample {
    pub task_id: String,
    pub task_title: String,
    pub source: String,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskTimingState {
    pub task_id: String,
    pub task_title: String,
    pub source: String,
    pub status: TaskStatus,
    pub status_started_at: u64,
    pub last_seen_at: u64,
    pub pending_started_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskTimingKind {
    Execution,
    Interruption,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedInterval {
    pub task_id: String,
    pub task_title: String,
    pub source: String,
    pub kind: TaskTimingKind,
    pub started_at: u64,
    pub ended_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskTimingUpdate {
    pub state: TaskTimingState,
    pub completed: Vec<CompletedInterval>,
}

fn new_state(sample: &TaskStatusSample, observed_at: u64) -> TaskTimingState {
    TaskTimingState {
        task_id: sample.task_id.clone(),
        task_title: sample.task_title.clone(),
        source: sample.source.clone(),
        status: sample.status,
        status_started_at: observed_at,
        last_seen_at: observed_at,
        pending_started_at: sample.status.is_pending().then_some(observed_at),
    }
}

fn interval_from_state(
    state: &TaskTimingState,
    kind: TaskTimingKind,
    started_at: u64,
    ended_at: u64,
) -> Option<CompletedInterval> {
    (ended_at > started_at).then(|| CompletedInterval {
        task_id: state.task_id.clone(),
        task_title: state.task_title.clone(),
        source: state.source.clone(),
        kind,
        started_at,
        ended_at,
    })
}

pub fn update_task_state(
    previous: Option<TaskTimingState>,
    sample: &TaskStatusSample,
    observed_at: u64,
    max_gap_ms: u64,
) -> TaskTimingUpdate {
    let Some(mut previous) = previous.filter(|state| state.task_id == sample.task_id) else {
        return TaskTimingUpdate { state: new_state(sample, observed_at), completed: Vec::new() };
    };
    let mut completed = Vec::new();
    if observed_at.saturating_sub(previous.last_seen_at) > max_gap_ms {
        if previous.status == TaskStatus::Executing {
            if let Some(interval) = interval_from_state(
                &previous,
                TaskTimingKind::Execution,
                previous.status_started_at,
                previous.last_seen_at,
            ) {
                completed.push(interval);
            }
        }
        return TaskTimingUpdate { state: new_state(sample, observed_at), completed };
    }

    if previous.status == sample.status {
        previous.task_title = sample.task_title.clone();
        previous.source = sample.source.clone();
        previous.last_seen_at = observed_at.max(previous.last_seen_at);
        return TaskTimingUpdate { state: previous, completed };
    }

    if previous.status == TaskStatus::Executing {
        if let Some(interval) = interval_from_state(
            &previous,
            TaskTimingKind::Execution,
            previous.status_started_at,
            observed_at,
        ) {
            completed.push(interval);
        }
    }

    let mut pending_started_at = previous.pending_started_at;
    if sample.status.is_pending() && pending_started_at.is_none() {
        pending_started_at = Some(observed_at);
    }
    if sample.status == TaskStatus::Executing {
        if let Some(started_at) = pending_started_at.take() {
            if let Some(interval) = interval_from_state(
                &previous,
                TaskTimingKind::Interruption,
                started_at,
                observed_at,
            ) {
                completed.push(interval);
            }
        }
    }

    TaskTimingUpdate {
        state: TaskTimingState {
            task_id: sample.task_id.clone(),
            task_title: sample.task_title.clone(),
            source: sample.source.clone(),
            status: sample.status,
            status_started_at: observed_at,
            last_seen_at: observed_at,
            pending_started_at,
        },
        completed,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TaskTimingSegment {
    pub kind: TaskTimingKind,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskTimingTaskSummary {
    pub task_id: String,
    pub task_title: String,
    pub source: String,
    pub execution_ms: u64,
    pub interruption_ms: u64,
    pub execution_count: u64,
    pub interruption_count: u64,
    pub segments: Vec<TaskTimingSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskTimingSummary {
    pub task_execution_ms: u64,
    pub actual_execution_ms: u64,
    pub tasks: Vec<TaskTimingTaskSummary>,
}

pub fn aggregate_task_timing(
    intervals: &[CompletedInterval],
    range_start: u64,
    range_end: u64,
) -> TaskTimingSummary {
    let mut tasks: HashMap<String, TaskTimingTaskSummary> = HashMap::new();
    let mut execution_ranges = Vec::new();
    let mut segments: HashMap<String, Vec<(u64, TaskTimingKind, u64)>> = HashMap::new();

    for interval in intervals {
        let started_at = interval.started_at.max(range_start);
        let ended_at = interval.ended_at.min(range_end);
        let duration = ended_at.saturating_sub(started_at);
        if duration == 0 {
            continue;
        }
        let task = tasks.entry(interval.task_id.clone()).or_insert_with(|| TaskTimingTaskSummary {
            task_id: interval.task_id.clone(),
            task_title: interval.task_title.clone(),
            source: interval.source.clone(),
            execution_ms: 0,
            interruption_ms: 0,
            execution_count: 0,
            interruption_count: 0,
            segments: Vec::new(),
        });
        task.task_title = interval.task_title.clone();
        task.source = interval.source.clone();
        segments
            .entry(interval.task_id.clone())
            .or_default()
            .push((started_at, interval.kind, duration));
        match interval.kind {
            TaskTimingKind::Execution => {
                task.execution_ms += duration;
                task.execution_count += 1;
                execution_ranges.push((started_at, ended_at));
            }
            TaskTimingKind::Interruption => {
                task.interruption_ms += duration;
                task.interruption_count += 1;
            }
        }
    }

    for (task_id, mut ordered) in segments {
        ordered.sort_by_key(|(started_at, _, _)| *started_at);
        if let Some(task) = tasks.get_mut(&task_id) {
            task.segments = ordered
                .into_iter()
                .map(|(_, kind, duration_ms)| TaskTimingSegment { kind, duration_ms })
                .collect();
        }
    }

    execution_ranges.sort_unstable();
    let mut actual_execution_ms = 0;
    let mut merged: Option<(u64, u64)> = None;
    for (start, end) in execution_ranges {
        match merged {
            Some((merged_start, merged_end)) if start <= merged_end => {
                merged = Some((merged_start, merged_end.max(end)));
            }
            Some((merged_start, merged_end)) => {
                actual_execution_ms += merged_end.saturating_sub(merged_start);
                merged = Some((start, end));
            }
            None => merged = Some((start, end)),
        }
    }
    if let Some((start, end)) = merged {
        actual_execution_ms += end.saturating_sub(start);
    }

    let mut tasks = tasks.into_values().collect::<Vec<_>>();
    tasks.sort_by(|a, b| {
        b.execution_ms
            .cmp(&a.execution_ms)
            .then_with(|| a.task_title.cmp(&b.task_title))
    });
    TaskTimingSummary {
        task_execution_ms: tasks.iter().map(|task| task.execution_ms).sum(),
        actual_execution_ms,
        tasks,
    }
}
