//! Annotations of task logs: kept as lines are appended ([`ActionsService::keep_annotations`],
//! from the masked lines), read for the run page ([`ActionsService::run_annotations`]) with the
//! step each one is in, so a page links to its line. Which lines are annotations, and how many a
//! task keeps, is `rbit_actions::logs::annotations`.
pub use rbit_actions::logs::{AnnotationLevel, AnnotationText, annotation_text};
use rbit_actions::logs::{AnnotationLine, MAX_ANNOTATIONS_PER_TASK, annotation_lines};
use rbit_core::actions::{Job, JobId, Step, TaskId};

use crate::{ActionsService, RepoContext, ServiceError};

/// One annotation of a job of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    pub job: JobId,
    pub level: AnnotationLevel,
    /// The stored text: the message, with ` (key=value,…)` when the command had parameters
    /// (`rbit_actions::logs::annotation_text` takes it apart).
    pub text: String,
    /// The step whose log holds it (its number, from 0) and its line in that step's log
    /// (from 1); `None` for a line outside every step.
    pub step: Option<(i64, u64)>,
}

/// The annotations of a run's jobs and the steps of their current tasks.
#[derive(Debug, Clone, Default)]
pub struct RunAnnotations {
    /// By job, in the order of `jobs`, then by line.
    pub annotations: Vec<Annotation>,
    pub steps: Vec<Step>,
}

impl ActionsService {
    /// Keeps `found` annotations of `task` ([`found`]). A failure is logged, never the
    /// append's: the lines are stored already.
    pub(in crate::actions) async fn keep_annotations(&self, task: TaskId, found: Vec<(u64, AnnotationLine)>) {
        if found.is_empty() {
            return;
        }
        let rows: Vec<(i64, &str, &str)> = found
            .iter()
            .map(|(line, annotation)| (i64::try_from(*line).unwrap_or(i64::MAX), annotation.level.as_str(), annotation.text.as_str()))
            .collect();
        let max = i64::try_from(MAX_ANNOTATIONS_PER_TASK).unwrap_or(i64::MAX);
        if let Err(error) = self.ctx().database.actions_runs().add_annotations(task, &rows, max).await {
            tracing::warn!(%error, task = task.0, "cannot keep a task's annotations");
        }
    }

    /// The annotations of `jobs` (a run of `repo` a reader opened) from their current tasks.
    pub async fn run_annotations(&self, repo: &RepoContext, jobs: &[Job]) -> Result<RunAnnotations, ServiceError> {
        self.require_pages(repo).await?;
        let jobs: Vec<&Job> = jobs.iter().filter(|job| job.repo_id == repo.repo.repo.id).collect();
        let tasks: Vec<TaskId> = jobs.iter().filter_map(|job| job.task_id).collect();
        if tasks.is_empty() {
            return Ok(RunAnnotations::default());
        }
        let store = self.ctx().database.actions_runs();
        let steps = store.steps_of_tasks(&tasks).await?;
        let stored = store.annotations_of_tasks(&tasks).await?;
        let mut annotations = Vec::new();
        for job in jobs {
            let Some(task) = job.task_id else { continue };
            let own: Vec<&Step> = steps.iter().filter(|step| step.task_id == task).collect();
            for row in stored.iter().filter(|row| row.task == task) {
                let Some(level) = AnnotationLevel::parse(&row.level) else { continue };
                annotations.push(Annotation { job: job.id, level, text: row.text.clone(), step: place(&own, row.line) });
            }
        }
        Ok(RunAnnotations { annotations, steps })
    }
}

/// The annotations among `lines` (masked, as stored), the first of which is task-log line
/// `first`, with their line numbers.
pub(in crate::actions) fn found(lines: &[String], first: u64) -> Vec<(u64, AnnotationLine)> {
    annotation_lines(lines.iter().map(String::as_str), first, MAX_ANNOTATIONS_PER_TASK)
}

/// The step holding task-log line `line`, and the line's number in it (from 1). A running
/// step reaches to the end of the log so far.
fn place(steps: &[&Step], line: i64) -> Option<(i64, u64)> {
    steps
        .iter()
        .filter(|step| step.started_at.is_some() && step.log_index <= line)
        .find(|step| step.status == rbit_core::actions::Status::Running || line < step.log_index.saturating_add(step.log_length))
        .map(|step| (step.number, u64::try_from(line - step.log_index).unwrap_or(0) + 1))
}

#[cfg(test)]
mod tests {
    use super::place;
    use rbit_core::{
        Timestamp,
        actions::{Status, Step, TaskId},
    };

    fn step(number: i64, status: Status, log_index: i64, log_length: i64) -> Step {
        Step {
            task_id: TaskId(1),
            number,
            name: format!("s{number}"),
            status,
            started_at: Some(Timestamp(1)),
            stopped_at: None,
            log_index,
            log_length,
        }
    }

    #[test]
    fn lines_are_placed_in_their_step() {
        let (first, second) = (step(0, Status::Failure, 2, 3), step(1, Status::Running, 5, 0));
        let steps = [&first, &second];
        assert_eq!(place(&steps, 0), None, "set-up lines before the first step");
        assert_eq!(place(&steps, 2), Some((0, 1)));
        assert_eq!(place(&steps, 4), Some((0, 3)));
        assert_eq!(place(&steps, 9), Some((1, 5)), "the running step reaches to the end");
        let finished = step(1, Status::Success, 5, 1);
        assert_eq!(place(&[&first, &finished], 9), None);
    }
}
