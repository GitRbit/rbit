//! Actions transition announcements; public payload builders are shared with the core webhook service.

use super::ActionsService;
use crate::{DomainEvent, ServiceError};
pub use rbit_core::actions::{JobAction, RunAction};
use rbit_core::{
    Timestamp,
    actions::{Job, RunId, Status},
};
use rbit_services_base::webhooks::actions::conclusion;
pub use rbit_services_base::webhooks::actions::{payload, people, workflow_file, workflow_job, workflow_run};

impl ActionsService {
    /// Publishes the `workflow_run` and `workflow_job` actions of run `id` that changed
    /// since they were last announced.
    pub async fn announce(&self, id: RunId) -> Result<(), ServiceError> {
        let store = self.ctx().database.actions_runs();
        let Some(run) = store.run(id).await? else { return Ok(()) };
        let jobs = store.jobs(id).await?;
        let running = jobs.iter().any(|job| job.status == Status::Running);
        let action = RunAction::of(run.status, running);
        // The run is requested before its jobs are announced, and completed after them.
        if action != RunAction::Completed && store.announce_run(id, action.as_str()).await? {
            self.ctx().events.publish(DomainEvent::ActionsRun { repo: run.repo_id, run: id, action });
        }
        for job in &jobs {
            let Some(job_action) = JobAction::of(job.status) else { continue };
            if store.announce_job(job.id, job_action.as_str()).await? {
                self.ctx().events.publish(DomainEvent::ActionsJob { repo: run.repo_id, run: id, job: job.id, action: job_action });
                record_job(job, job_action);
            }
        }
        if action == RunAction::Completed {
            // A run that finished without anyone seeing it start (a workflow error, or every
            // job cancelled at once) is requested first, as GitHub always sends both.
            if store.announce_run(id, RunAction::Requested.as_str()).await? {
                self.ctx().events.publish(DomainEvent::ActionsRun { repo: run.repo_id, run: id, action: RunAction::Requested });
            }
            if store.announce_run(id, action.as_str()).await? {
                self.ctx().events.publish(DomainEvent::ActionsRun { repo: run.repo_id, run: id, action });
                // Actions: metrics.
                let outcome = conclusion(run.status).unwrap_or("failure");
                metrics::counter!("rbit_actions_runs_total", "event" => run.event.as_str(), "outcome" => outcome).increment(1);
            }
        }
        Ok(())
    }
}

// Actions: metrics.
/// Records what `job` just did, announced once: the time it waited for a runner when one
/// takes it, and its outcome and how long it ran when it finishes.
fn record_job(job: &Job, action: JobAction) {
    #[allow(clippy::cast_precision_loss)] // Seconds; precision beyond 2^53 of them is moot.
    let seconds = |from: Timestamp, to: Timestamp| (to.seconds() - from.seconds()).max(0) as f64;
    match action {
        JobAction::InProgress => {
            if let Some(started) = job.started_at {
                let waited = seconds(job.queued_at.unwrap_or(job.created_at), started);
                metrics::histogram!("rbit_actions_job_queue_seconds").record(waited);
            }
        }
        JobAction::Completed => {
            let outcome = conclusion(job.status).unwrap_or("failure");
            metrics::counter!("rbit_actions_jobs_total", "outcome" => outcome).increment(1);
            if let (Some(started), Some(finished)) = (job.started_at, job.finished_at) {
                metrics::histogram!("rbit_actions_job_duration_seconds").record(seconds(started, finished));
            }
        }
        JobAction::Waiting | JobAction::Queued => {}
    }
}
