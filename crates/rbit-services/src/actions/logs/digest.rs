//! The failure digest of a run: for each job that did not succeed, how it ended, the error
//! annotations its log carried, and the last lines of the step that ended it, cleaned and
//! bounded. It is what an agent reads instead of a whole log (agent plan step 1.5).
//!
//! The step is the one the termination record names; without one (an older runner, a lost
//! runner), the last step that failed, else the last step that ran. The excerpt is read from
//! the stored, masked log, tail first, so its cost does not grow with the log.

use rbit_actions::logs::{EXCERPT_LINES, Excerpt, annotation_text, excerpt};
use rbit_core::actions::{Job, JobId, Run, RunId, Status, Step, TaskId, Termination, TerminationReason};

use rbit_storage::sqlite::actions_runs::{StoredAnnotation, TaskEnd};

use super::LogRange;
use crate::{ActionsService, RepoContext, ServiceError};

/// Raw lines read from the end of a step to build its excerpt.
const EXCERPT_READ_LINES: usize = 400;
/// Bytes read from the end of a step to build its excerpt.
const EXCERPT_READ_BYTES: usize = 128 * 1024;
/// Error annotations listed per job.
pub const DIGEST_ANNOTATIONS: usize = 10;

/// One annotation in a digest: where it points and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestAnnotation {
    pub path: Option<String>,
    pub line: Option<u64>,
    pub message: String,
}

/// A job that did not succeed.
#[derive(Debug, Clone)]
pub struct FailedJob {
    pub job: Job,
    /// The termination record, or what the job's status implies.
    pub termination: Termination,
    pub log_complete: bool,
    /// The step the excerpt is from.
    pub step: Option<Step>,
    pub annotations: Vec<DigestAnnotation>,
    pub excerpt: Option<Excerpt>,
}

/// A run's failure digest.
#[derive(Debug, Clone)]
pub struct FailureDigest {
    pub run: Run,
    pub failed: Vec<FailedJob>,
}

impl ActionsService {
    /// The failure digest of run `id` of `repo`: empty `failed` for a run with no failed job.
    pub async fn failure_digest(&self, repo: &RepoContext, id: RunId) -> Result<FailureDigest, ServiceError> {
        let detail = self.run_by_id(repo, id).await?;
        let failed_jobs: Vec<Job> = detail
            .jobs
            .into_iter()
            .filter(|job| matches!(job.status, Status::Failure | Status::Cancelled) && job.task_id.is_some())
            .collect();
        let ends = self.task_ends(&failed_jobs).await?;
        let steps = self.steps(&failed_jobs).await?;
        let store = self.ctx().database.actions_runs();
        let tasks: Vec<TaskId> = failed_jobs.iter().filter_map(|job| job.task_id).collect();
        let annotations = store.annotations_of_tasks(&tasks).await?;
        let mut failed = Vec::with_capacity(failed_jobs.len());
        for job in failed_jobs {
            let Some(task) = job.task_id else { continue };
            let end = ends.get(&task);
            let termination = end.and_then(|end| end.termination).unwrap_or_else(|| {
                Termination::because(if job.status == Status::Cancelled {
                    TerminationReason::Cancelled
                } else {
                    TerminationReason::Unknown
                })
            });
            let own: Vec<&Step> = steps.iter().filter(|step| step.task_id == task).collect();
            let step = pick_step(&own, termination).cloned();
            let excerpt = match &step {
                Some(step) => {
                    let window = self.step_log(repo, task, step.number, LogRange::Tail, EXCERPT_READ_LINES, EXCERPT_READ_BYTES).await?;
                    Some(excerpt(&window.lines, EXCERPT_LINES))
                }
                None => None,
            };
            let annotations = annotations
                .iter()
                .filter(|annotation| annotation.task == task && annotation.level == "error")
                .take(DIGEST_ANNOTATIONS)
                .map(|annotation| {
                    let text = annotation_text(&annotation.text);
                    DigestAnnotation {
                        path: text.file.map(str::to_owned),
                        line: text.line.and_then(|line| line.parse().ok()),
                        message: text.message.to_owned(),
                    }
                })
                .collect();
            failed.push(FailedJob { job, termination, log_complete: end.is_some_and(|end| end.log_complete), step, annotations, excerpt });
        }
        Ok(FailureDigest { run: detail.run, failed })
    }

    /// The task of attempt `attempt` of job `id` (the current one when `None`), for a reader
    /// of `repo`, with every attempt the job has had.
    pub async fn job_attempt(
        &self,
        repo: &RepoContext,
        id: JobId,
        attempt: Option<i64>,
    ) -> Result<(TaskId, Vec<(TaskId, i64)>), ServiceError> {
        let detail = self.job_detail(repo, id).await?;
        let attempts = self.ctx().database.actions_runs().job_tasks(detail.job.id).await?;
        let task = match attempt {
            Some(wanted) => attempts.iter().find(|(_, number)| *number == wanted).map(|(task, _)| *task),
            None => detail.job.task_id,
        };
        Ok((task.ok_or(ServiceError::NotFound)?, attempts))
    }

    /// The annotations of attempt `attempt` (the current one when `None`) of job `id`.
    pub async fn job_annotations(
        &self,
        repo: &RepoContext,
        id: JobId,
        attempt: Option<i64>,
    ) -> Result<Vec<StoredAnnotation>, ServiceError> {
        let (task, _) = self.job_attempt(repo, id, attempt).await?;
        Ok(self.ctx().database.actions_runs().annotations_of_tasks(&[task]).await?)
    }

    /// Every attempt of job `id`, oldest first: its number, whether it is the current one,
    /// and how it ended.
    pub async fn job_attempts(&self, repo: &RepoContext, id: JobId) -> Result<Vec<(i64, bool, Option<TaskEnd>)>, ServiceError> {
        let (current, attempts) = self.job_attempt(repo, id, None).await?;
        let tasks: Vec<TaskId> = attempts.iter().map(|(task, _)| *task).collect();
        let ends = self.ctx().database.actions_runs().task_ends(&tasks).await?;
        Ok(attempts.into_iter().map(|(task, attempt)| (attempt, task == current, ends.get(&task).copied())).collect())
    }
}

/// The step a digest shows: the one the record names, else the last failed one, else the
/// last one that ran.
fn pick_step<'a>(steps: &[&'a Step], termination: Termination) -> Option<&'a Step> {
    let named = termination.step.and_then(|index| steps.iter().find(|step| u32::try_from(step.number).ok() == Some(index)));
    named
        .or_else(|| steps.iter().rev().find(|step| step.status == Status::Failure))
        .or_else(|| steps.iter().rev().find(|step| step.started_at.is_some()))
        .copied()
}

#[cfg(test)]
mod tests {
    use rbit_core::{Timestamp, actions::Status};

    use super::*;

    fn step(number: i64, status: Status, started: bool) -> Step {
        Step {
            task_id: TaskId(1),
            number,
            name: format!("step {number}"),
            status,
            started_at: started.then_some(Timestamp(1)),
            stopped_at: None,
            log_index: 0,
            log_length: 0,
        }
    }

    #[test]
    fn the_named_step_wins_then_the_last_failure_then_the_last_that_ran() {
        let steps = [
            step(0, Status::Success, true),
            step(1, Status::Failure, true),
            step(2, Status::Success, true),
            step(3, Status::Skipped, false),
        ];
        let refs: Vec<&Step> = steps.iter().collect();
        let named = Termination { reason: TerminationReason::StepFailed, step: Some(2), exit_code: Some(1) };
        assert_eq!(pick_step(&refs, named).map(|step| step.number), Some(2));
        assert_eq!(pick_step(&refs, Termination::because(TerminationReason::Unknown)).map(|step| step.number), Some(1));
        let lost = [step(0, Status::Success, true), step(1, Status::Cancelled, true), step(2, Status::Skipped, false)];
        let refs: Vec<&Step> = lost.iter().collect();
        assert_eq!(pick_step(&refs, Termination::because(TerminationReason::RunnerLost)).map(|step| step.number), Some(1));
        assert!(pick_step(&[], Termination::because(TerminationReason::SetupFailed)).is_none());
    }
}
