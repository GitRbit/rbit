//! Commit statuses for jobs: context naming and the system-actor bridge into
//! `StatusService`.
//!
//! * One status per job on the run's commit: context `"<workflow> / <job> (<event>)"`
//!   ([`status_context`], at most 255 characters), the job page as `target_url`
//!   ([`job_url`]: `/{owner}/{repo}/actions/runs/{run number}/jobs/{job id}`) and a
//!   description by state ([`describe`]): "Waiting for approval", "Queued", "In progress",
//!   "Successful in 1m 3s", "Failing after 12s", "Cancelled", "Skipped".
//! * They are posted as rbit Actions (`StatusService::create_as_system`: no creator,
//!   `source = actions`, webhook sender `rbit-actions[bot]`) whenever a job is queued,
//!   starts or finishes: [`ActionsService::report_statuses`] runs after every transition,
//!   and a report equal to the context's current one is not repeated, so heartbeats and
//!   repeated settles post nothing.
//! * Each status publishes `DomainEvent::CommitStatus`; the pull request service
//!   re-evaluates the pull requests whose head the commit is, so required checks follow.
//! * Scheduled runs post none — no commit is being judged — and workflow errors have no
//!   jobs.
//!
//! Workstream T (triggers and runs) owns this file.

use rbit_core::{
    actions::{Job, JobId, Run, RunEvent, RunId, Status},
    repository::OwnedRepository,
    status::{MAX_CONTEXT_CHARS, StatusReport, StatusState},
};

use super::{ActionsService, runs::truncate_chars};
use crate::{ServiceError, StatusService};

/// The context of `job`'s statuses: `"<workflow> / <job> (<event>)"`, at most 255
/// characters.
pub fn status_context(workflow: &str, job: &str, event: RunEvent) -> String {
    truncate_chars(&format!("{workflow} / {job} ({event})"), MAX_CONTEXT_CHARS)
}

/// The job page a status links to.
pub fn job_url(base_url: &str, repo: &OwnedRepository, run_number: i64, job: JobId) -> String {
    format!("{base_url}/{}/actions/runs/{run_number}/jobs/{}", repo.full_name(), job.0)
}

/// The state and description a job reports.
pub fn describe(job: &Job) -> (StatusState, String) {
    let took = match (job.started_at, job.finished_at) {
        (Some(started), Some(finished)) => Some(duration(finished.seconds() - started.seconds())),
        _ => None,
    };
    match job.status {
        Status::AwaitingApproval => (StatusState::Pending, "Waiting for approval".to_owned()),
        Status::Waiting | Status::Blocked => (StatusState::Pending, "Queued".to_owned()),
        Status::Running => (StatusState::Pending, "In progress".to_owned()),
        Status::Success => (StatusState::Success, took.map_or_else(|| "Successful".to_owned(), |took| format!("Successful in {took}"))),
        Status::Skipped => (StatusState::Success, "Skipped".to_owned()),
        Status::Failure => (StatusState::Failure, took.map_or_else(|| "Failing".to_owned(), |took| format!("Failing after {took}"))),
        Status::Cancelled => (StatusState::Error, "Cancelled".to_owned()),
    }
}

/// `1h 2m`, `1m 3s` or `12s`.
pub fn duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let (hours, minutes, rest) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    match (hours, minutes) {
        (0, 0) => format!("{rest}s"),
        (0, _) => format!("{minutes}m {rest}s"),
        _ => format!("{hours}h {minutes}m"),
    }
}

impl ActionsService {
    /// Posts the current status of every job of run `id` whose report changed. Called after
    /// every transition this module makes; the runner protocol calls it when a job starts
    /// (and, through [`ActionsService::settled`], when one finishes).
    pub async fn report_statuses(&self, id: RunId) -> Result<(), ServiceError> {
        let database = &self.ctx().database;
        let Some(run) = database.actions_runs().run(id).await? else { return Ok(()) };
        if run.event == RunEvent::Schedule || run.is_workflow_error() {
            return Ok(());
        }
        let Some(repo) = database.admin().find_repository(run.repo_id).await? else { return Ok(()) };
        let jobs = database.actions_runs().jobs(run.id).await?;
        let current = database.commit_statuses().latest(run.repo_id, &run.sha).await?;
        let statuses = StatusService::new(self.ctx().clone());
        for job in &jobs {
            let Some(report) = self.report(&repo, &run, job) else { continue };
            let unchanged = current
                .iter()
                .any(|status| status.context == report.context && status.state == report.state && status.description == report.description);
            if !unchanged {
                statuses.create_as_system(run.repo_id, &run.sha, &report).await?;
            }
        }
        Ok(())
    }

    fn report(&self, repo: &OwnedRepository, run: &Run, job: &Job) -> Option<StatusReport> {
        let context = status_context(&run.workflow_name, &job.name, run.event);
        let (state, description) = describe(job);
        let url = job_url(self.ctx().config.server.base_url(), repo, run.number, job.id);
        // A public URL that is no absolute http(s) URL only loses the link.
        StatusReport::parse(state.as_str(), Some(&context), Some(&description), Some(&url))
            .or_else(|_| StatusReport::parse(state.as_str(), Some(&context), Some(&description), None))
            .inspect_err(|error| tracing::warn!(%error, context, "cannot report a job's status"))
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{RepoId, Timestamp};

    use super::*;

    fn job(status: Status, started: Option<i64>, finished: Option<i64>) -> Job {
        Job {
            id: JobId(5),
            run_id: RunId(1),
            repo_id: RepoId(1),
            key: "build".into(),
            name: "build (linux)".into(),
            matrix: None,
            needs: Vec::new(),
            runs_on: vec!["linux".into()],
            permissions: None,
            status,
            attempt: 1,
            task_id: None,
            runner_id: None,
            created_at: Timestamp(0),
            queued_at: None,
            started_at: started.map(Timestamp),
            finished_at: finished.map(Timestamp),
        }
    }

    #[test]
    fn contexts_name_the_workflow_job_and_event() {
        assert_eq!(status_context("CI", "build (linux)", RunEvent::PullRequest), "CI / build (linux) (pull_request)");
        let long = status_context(&"w".repeat(300), "j", RunEvent::Push);
        assert_eq!(long.chars().count(), MAX_CONTEXT_CHARS);
        assert!(StatusReport::parse("pending", Some(&long), None, None).is_ok());
        let wide = status_context(&"é".repeat(300), "j", RunEvent::Push);
        assert_eq!(wide.chars().count(), MAX_CONTEXT_CHARS, "characters, not bytes");
    }

    #[test]
    fn descriptions_follow_the_job() {
        let table = [
            (job(Status::AwaitingApproval, None, None), StatusState::Pending, "Waiting for approval"),
            (job(Status::Waiting, None, None), StatusState::Pending, "Queued"),
            (job(Status::Blocked, None, None), StatusState::Pending, "Queued"),
            (job(Status::Running, Some(10), None), StatusState::Pending, "In progress"),
            (job(Status::Success, Some(10), Some(73)), StatusState::Success, "Successful in 1m 3s"),
            (job(Status::Success, None, Some(73)), StatusState::Success, "Successful"),
            (job(Status::Skipped, None, Some(1)), StatusState::Success, "Skipped"),
            (job(Status::Failure, Some(10), Some(22)), StatusState::Failure, "Failing after 12s"),
            (job(Status::Cancelled, None, Some(5)), StatusState::Error, "Cancelled"),
        ];
        for (job, state, description) in table {
            assert_eq!(describe(&job), (state, description.to_owned()), "{:?}", job.status);
        }
        assert_eq!([duration(0), duration(59), duration(60), duration(3725), duration(-3)], ["0s", "59s", "1m 0s", "1h 2m", "0s"]);
    }
}
