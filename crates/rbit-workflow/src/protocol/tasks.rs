//! Protocol tasks contracts and original validation.
use super::limits::{MAX_CAPACITY, MAX_ENVIRONMENT_URL_BYTES, MAX_POST_STEP_NAME_BYTES, MAX_STEP_SUMMARY_BYTES, MAX_TASK_SUMMARY_BYTES};
use super::validation::{ValidationError, ensure, unique, validate_outputs};
use crate::spec::JobSpec;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// A task: one attempt of one job on one runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(pub i64);

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// `POST poll`, sent with an [`crate::protocol::IDEMPOTENCY_KEY_HEADER`] and only while a slot is free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollRequest {
    pub free_slots: u32,
}

impl PollRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        ensure((1..=MAX_CAPACITY).contains(&self.free_slots), || format!("`free_slots` must be between 1 and {MAX_CAPACITY}"))
    }
}

/// A claimed task. A poll that ends without one answers `204` and no body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollResponse {
    pub task: JobSpec,
}

/// Where a step is in its life cycle; the wire names of `rbit_core::actions::Status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Waiting,
    Running,
    Success,
    Failure,
    Cancelled,
    Skipped,
}

impl StepStatus {
    pub const fn is_done(self) -> bool {
        matches!(self, Self::Success | Self::Failure | Self::Cancelled | Self::Skipped)
    }
}

/// One step of a task as the runner last saw it. Its log is the task log's lines
/// `log_index .. log_index + log_length`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepState {
    /// The step's position in [`crate::spec::JobTemplate::steps`], from 0.
    pub index: u32,
    pub status: StepStatus,
    pub started_at: Option<i64>,
    pub stopped_at: Option<i64>,
    pub log_index: u64,
    pub log_length: u64,
}

/// How a task ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskResult {
    Success,
    Failure,
    Cancelled,
}

/// `POST tasks/{id}/state`: the heartbeat, step states and outputs, and the result once the
/// task has ended. Steps are keyed by index and outputs are kept when first seen, so a
/// resent call changes nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStateRequest {
    pub steps: Vec<StepState>,
    /// Job outputs evaluated so far, masked by the runner.
    pub outputs: BTreeMap<String, String>,
    /// Set on the final call only.
    pub result: Option<TaskResult>,
    // Actions: step summaries.
    /// What steps wrote to `GITHUB_STEP_SUMMARY` (Markdown, masked by the runner), by step
    /// index, once each step has ended. The first summary seen for a step is kept, so a
    /// resent call changes nothing. Left out when empty, as older runners send it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub summaries: BTreeMap<u32, String>,
    // Actions: third-party actions.
    /// Steps the runner added after the job's own, by index: the `post` scripts of the
    /// actions it ran (`Post Run actions/setup-node@v4`). The server creates them once; an
    /// index the template already has keeps its name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub post_steps: BTreeMap<u32, String>,
    // Actions: environments.
    /// The job's `environment.url`, as the runner evaluated it after the steps (masked).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_url: Option<String>,
}

impl TaskStateRequest {
    pub fn is_final(&self) -> bool {
        self.result.is_some()
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        // Every step of the template may add a post step.
        let max_steps = 2 * crate::spec::MAX_STEPS;
        ensure(self.steps.len() <= max_steps, || format!("a task has at most {max_steps} steps"))?;
        for (index, name) in &self.post_steps {
            ensure(usize::try_from(*index).is_ok_and(|index| index < max_steps), || format!("post step {index} is out of range"))?;
            ensure(!name.is_empty() && name.len() <= MAX_POST_STEP_NAME_BYTES && !name.chars().any(char::is_control), || {
                format!("post step {index} needs a name of 1 to {MAX_POST_STEP_NAME_BYTES} bytes")
            })?;
        }
        ensure(unique(self.steps.iter().map(|step| step.index)), || "step indexes must not repeat".to_owned())?;
        for step in &self.steps {
            let index = step.index;
            ensure(usize::try_from(index).is_ok_and(|index| index < max_steps), || format!("step {index} is out of range"))?;
            ensure(step.log_index.checked_add(step.log_length).is_some(), || format!("step {index} has an impossible log range"))?;
            if let (Some(started), Some(stopped)) = (step.started_at, step.stopped_at) {
                ensure(stopped >= started, || format!("step {index} stopped before it started"))?;
            }
            ensure(!self.is_final() || step.status != StepStatus::Running, || format!("step {index} is still running"))?;
        }
        // Actions: step summaries.
        let mut total = 0usize;
        for (index, summary) in &self.summaries {
            ensure(usize::try_from(*index).is_ok_and(|index| index < max_steps), || {
                format!("the summary of step {index} is out of range")
            })?;
            ensure(summary.len() <= MAX_STEP_SUMMARY_BYTES, || {
                format!("the summary of step {index} is over {MAX_STEP_SUMMARY_BYTES} bytes")
            })?;
            total = total.saturating_add(summary.len());
        }
        ensure(total <= MAX_TASK_SUMMARY_BYTES, || format!("a task's step summaries may be at most {MAX_TASK_SUMMARY_BYTES} bytes"))?;
        if let Some(url) = &self.environment_url {
            ensure(url.len() <= MAX_ENVIRONMENT_URL_BYTES && !url.chars().any(char::is_control), || {
                format!("the environment URL must be at most {MAX_ENVIRONMENT_URL_BYTES} bytes, on one line")
            })?;
        }
        validate_outputs(&self.outputs)
    }
}

/// The answer to every state call: whether the job was cancelled. Cancellation reaches a
/// runner this way; by then its task token is already dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStateResponse {
    pub cancel: bool,
}

/// The query of `POST tasks/{id}/logs`: the task-log line the body's first line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogsQuery {
    pub from: u64,
}

/// The answer to a log append: the server's line count after it, which is where the next
/// append starts. Lines before it that were sent again are dropped, so a resend is harmless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogsResponse {
    pub next_line: u64,
}
