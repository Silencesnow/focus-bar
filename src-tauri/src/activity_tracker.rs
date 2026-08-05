use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivitySource {
    Cmux,
    Codex,
    Chrome,
    Vscode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityType {
    AiInput,
    AiReading,
    BrowserReview,
    CodeReading,
    CodeEditing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub source: ActivitySource,
    pub activity_type: ActivityType,
    pub task_id: Option<String>,
    pub task_title: Option<String>,
    pub confidence: Confidence,
    pub context_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivitySegment {
    pub started_at: u64,
    pub ended_at: u64,
    pub source: ActivitySource,
    pub activity_type: ActivityType,
    pub task_id: Option<String>,
    pub task_title: Option<String>,
    pub confidence: Confidence,
    pub context_key: Option<String>,
}

impl ActivitySegment {
    fn matches(&self, observation: &Observation) -> bool {
        self.source == observation.source
            && self.activity_type == observation.activity_type
            && self.task_id == observation.task_id
            && self.confidence == observation.confidence
            && self.context_key == observation.context_key
    }
}

pub struct SegmentTracker {
    idle_timeout_ms: u64,
    min_segment_ms: u64,
    current: Option<ActivitySegment>,
}

impl SegmentTracker {
    pub fn new(idle_timeout_ms: u64, min_segment_ms: u64) -> Self {
        Self { idle_timeout_ms, min_segment_ms, current: None }
    }

    pub fn current(&self) -> Option<&ActivitySegment> {
        self.current.as_ref()
    }

    pub fn update(
        &mut self,
        observation: Option<Observation>,
        last_user_activity_at: u64,
        now: u64,
    ) -> Vec<ActivitySegment> {
        if now.saturating_sub(last_user_activity_at) >= self.idle_timeout_ms {
            return self.close(last_user_activity_at.saturating_add(self.idle_timeout_ms));
        }

        match observation {
            Some(observation) => {
                if self.current.as_ref().is_some_and(|current| current.matches(&observation)) {
                    if let Some(current) = self.current.as_mut() {
                        current.ended_at = now;
                    }
                    Vec::new()
                } else {
                    let closed = self.close(now);
                    self.current = Some(ActivitySegment {
                        started_at: now,
                        ended_at: now,
                        source: observation.source,
                        activity_type: observation.activity_type,
                        task_id: observation.task_id,
                        task_title: observation.task_title,
                        confidence: observation.confidence,
                        context_key: observation.context_key,
                    });
                    closed
                }
            }
            None => self.close(now),
        }
    }

    pub fn close(&mut self, ended_at: u64) -> Vec<ActivitySegment> {
        let Some(mut current) = self.current.take() else {
            return Vec::new();
        };
        current.ended_at = ended_at.max(current.started_at);
        if current.ended_at.saturating_sub(current.started_at) < self.min_segment_ms {
            Vec::new()
        } else {
            vec![current]
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DurationBreakdown {
    pub key: String,
    pub total_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskActivitySummary {
    pub task_id: Option<String>,
    pub task_title: String,
    pub total_ms: u64,
    pub by_source: Vec<DurationBreakdown>,
    pub by_activity: Vec<DurationBreakdown>,
    pub by_confidence: Vec<DurationBreakdown>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivitySummary {
    pub total_ms: u64,
    pub tasks: Vec<TaskActivitySummary>,
    pub by_source: Vec<DurationBreakdown>,
    pub by_activity: Vec<DurationBreakdown>,
    pub by_confidence: Vec<DurationBreakdown>,
}

fn sorted_breakdown(values: HashMap<String, u64>) -> Vec<DurationBreakdown> {
    let mut values = values
        .into_iter()
        .map(|(key, total_ms)| DurationBreakdown { key, total_ms })
        .collect::<Vec<_>>();
    values.sort_by(|a, b| b.total_ms.cmp(&a.total_ms).then_with(|| a.key.cmp(&b.key)));
    values
}

pub fn aggregate_segments(segments: &[ActivitySegment], range_start: u64, range_end: u64) -> ActivitySummary {
    let mut total_ms = 0;
    let mut by_source = HashMap::new();
    let mut by_activity = HashMap::new();
    let mut by_confidence = HashMap::new();
    let mut tasks: HashMap<Option<String>, (String, u64, HashMap<String, u64>, HashMap<String, u64>, HashMap<String, u64>)> = HashMap::new();

    for segment in segments {
        let start = segment.started_at.max(range_start);
        let end = segment.ended_at.min(range_end);
        let duration = end.saturating_sub(start);
        if duration == 0 {
            continue;
        }
        let source = serde_json::to_value(segment.source).unwrap().as_str().unwrap().to_string();
        let activity = serde_json::to_value(segment.activity_type).unwrap().as_str().unwrap().to_string();
        let confidence = serde_json::to_value(segment.confidence).unwrap().as_str().unwrap().to_string();
        total_ms += duration;
        *by_source.entry(source.clone()).or_insert(0) += duration;
        *by_activity.entry(activity.clone()).or_insert(0) += duration;
        *by_confidence.entry(confidence.clone()).or_insert(0) += duration;
        let title = segment.task_title.clone().unwrap_or_else(|| "未归属".into());
        let task = tasks.entry(segment.task_id.clone()).or_insert_with(|| (title, 0, HashMap::new(), HashMap::new(), HashMap::new()));
        task.1 += duration;
        *task.2.entry(source).or_insert(0) += duration;
        *task.3.entry(activity).or_insert(0) += duration;
        *task.4.entry(confidence).or_insert(0) += duration;
    }

    let mut tasks = tasks
        .into_iter()
        .map(|(task_id, (task_title, total_ms, sources, activities, confidence))| TaskActivitySummary {
            task_id,
            task_title,
            total_ms,
            by_source: sorted_breakdown(sources),
            by_activity: sorted_breakdown(activities),
            by_confidence: sorted_breakdown(confidence),
        })
        .collect::<Vec<_>>();
    tasks.sort_by(|a, b| b.total_ms.cmp(&a.total_ms).then_with(|| a.task_title.cmp(&b.task_title)));

    ActivitySummary {
        total_ms,
        tasks,
        by_source: sorted_breakdown(by_source),
        by_activity: sorted_breakdown(by_activity),
        by_confidence: sorted_breakdown(by_confidence),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(task: &str, activity_type: ActivityType) -> Observation {
        Observation {
            source: ActivitySource::Codex,
            activity_type,
            task_id: Some(task.into()),
            task_title: Some(format!("Task {task}")),
            confidence: Confidence::High,
            context_key: Some(format!("thread-{task}")),
        }
    }

    #[test]
    fn context_change_closes_the_previous_segment() {
        let mut tracker = SegmentTracker::new(90_000, 2_000);
        assert!(tracker.update(Some(observation("a", ActivityType::AiReading)), 0, 0).is_empty());

        let closed = tracker.update(Some(observation("b", ActivityType::AiReading)), 5_000, 5_000);

        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].task_id.as_deref(), Some("a"));
        assert_eq!(closed[0].started_at, 0);
        assert_eq!(closed[0].ended_at, 5_000);
    }

    #[test]
    fn idle_time_is_capped_at_ninety_seconds_after_last_activity() {
        let mut tracker = SegmentTracker::new(90_000, 2_000);
        tracker.update(Some(observation("a", ActivityType::AiReading)), 10_000, 10_000);

        let closed = tracker.update(Some(observation("a", ActivityType::AiReading)), 10_000, 101_000);

        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].ended_at, 100_000);
        assert!(tracker.current().is_none());
    }

    #[test]
    fn input_and_reading_become_separate_segments() {
        let mut tracker = SegmentTracker::new(90_000, 2_000);
        tracker.update(Some(observation("a", ActivityType::AiReading)), 0, 0);

        let closed = tracker.update(Some(observation("a", ActivityType::AiInput)), 8_000, 8_000);

        assert_eq!(closed[0].activity_type, ActivityType::AiReading);
        assert_eq!(tracker.current().unwrap().activity_type, ActivityType::AiInput);
    }

    #[test]
    fn segments_shorter_than_two_seconds_are_discarded() {
        let mut tracker = SegmentTracker::new(90_000, 2_000);
        tracker.update(Some(observation("a", ActivityType::AiReading)), 0, 0);

        assert!(tracker.update(None, 1_000, 1_000).is_empty());
    }

    #[test]
    fn aggregation_clips_segments_to_the_requested_range() {
        let segments = vec![ActivitySegment {
            started_at: 5_000,
            ended_at: 15_000,
            source: ActivitySource::Codex,
            activity_type: ActivityType::AiReading,
            task_id: Some("a".into()),
            task_title: Some("Task A".into()),
            confidence: Confidence::High,
            context_key: Some("thread-a".into()),
        }];

        let summary = aggregate_segments(&segments, 10_000, 20_000);

        assert_eq!(summary.total_ms, 5_000);
        assert_eq!(summary.tasks[0].total_ms, 5_000);
        assert_eq!(summary.tasks[0].task_id.as_deref(), Some("a"));
    }
}
