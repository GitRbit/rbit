//! `workflow_run` and `workflow_job` webhooks (§4.11 of `docs/design/actions.md`).
//!
//! Every transition of a run's jobs ends in [`ActionsService::report_statuses`], which calls
//! [`ActionsService::announce`] first: it compares what the run and each job show now with
//! the action last announced for them (`announced` columns of `actions_runs` and
//! `actions_jobs`) and publishes a [`DomainEvent::ActionsRun`] or
//! [`DomainEvent::ActionsJob`] for each change. The compare-and-set in the store makes each
//! action go out once, however many callers race. [`payload`] then builds GitHub's shapes
//! (subsets) for [`crate::WebhookService::handle_event`]. The REST API returns runs and
//! jobs with the same builders, [`workflow_run`] and [`workflow_job`], so the two never
//! disagree.
//!
//! | Event | Action | When |
//! |---|---|---|
//! | `workflow_run` | `requested` | the run exists (again, after a re-run) and nothing runs yet |
//! | `workflow_run` | `in_progress` | a job of it runs |
//! | `workflow_run` | `completed` | it finished; `conclusion` says how |
//! | `workflow_job` | `waiting` | its run awaits approval |
//! | `workflow_job` | `queued` | it waits for a runner |
//! | `workflow_job` | `in_progress` | a runner took it |
//! | `workflow_job` | `completed` | it finished (skipped jobs too) |
//!
//! A job blocked on `needs` or a concurrency group is announced once it is queued. With
//! Actions disabled there is no service, so nothing is announced and stored subscriptions
//! never fire.
//!
//! The integration workstream (I) owns this file.

use std::collections::BTreeMap;

use rbit_core::{
    Account, AccountId,
    actions::{Job, Run, RunEvent, RunId, Status, Step},
    repository::OwnedRepository,
    webhook::WebhookEvent,
};
use serde_json::{Value, json};

use super::{ActionsService, statuses::job_url};
use crate::{
    DomainEvent, ServiceContext, ServiceError,
    statuses::hooks::actions_bot,
    webhooks::{
        HookPayload,
        payloads::{account, repository, timestamp, with_organization},
    },
};

/// A `workflow_run` action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunAction {
    Requested,
    InProgress,
    Completed,
}

impl RunAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }

    /// What a run with `status` shows; `running` says whether a job of it runs.
    pub const fn of(status: Status, running: bool) -> Self {
        if status.is_done() {
            Self::Completed
        } else if running {
            Self::InProgress
        } else {
            Self::Requested
        }
    }
}

/// A `workflow_job` action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobAction {
    Waiting,
    Queued,
    InProgress,
    Completed,
}

impl JobAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Queued => "queued",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }

    /// What a job with `status` shows; `None` while it is blocked on `needs` or a group.
    pub const fn of(status: Status) -> Option<Self> {
        match status {
            Status::AwaitingApproval => Some(Self::Waiting),
            Status::Waiting => Some(Self::Queued),
            Status::Running => Some(Self::InProgress),
            Status::Blocked => None,
            Status::Success | Status::Failure | Status::Cancelled | Status::Skipped => Some(Self::Completed),
        }
    }
}

/// GitHub's `status` field: `queued`, `waiting`, `in_progress` or `completed`.
const fn github_status(status: Status) -> &'static str {
    match status {
        Status::Waiting | Status::Blocked => "queued",
        Status::AwaitingApproval => "waiting",
        Status::Running => "in_progress",
        Status::Success | Status::Failure | Status::Cancelled | Status::Skipped => "completed",
    }
}

/// GitHub's `conclusion` field, `null` until finished.
const fn conclusion(status: Status) -> Option<&'static str> {
    match status {
        Status::Success => Some("success"),
        Status::Failure => Some("failure"),
        Status::Cancelled => Some("cancelled"),
        Status::Skipped => Some("skipped"),
        Status::Waiting | Status::Running | Status::Blocked | Status::AwaitingApproval => None,
    }
}

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
            }
        }
        Ok(())
    }
}

/// The `workflow_run` or `workflow_job` payload of `event`, or `None` for other events and
/// for runs, jobs or repositories that are gone.
pub async fn payload(ctx: &ServiceContext, event: &DomainEvent, base_url: &str) -> Result<Option<HookPayload>, ServiceError> {
    let (repo, run, job, action, kind) = match event {
        DomainEvent::ActionsRun { repo, run, action } => (*repo, *run, None, action.as_str(), WebhookEvent::WorkflowRun),
        DomainEvent::ActionsJob { repo, run, job, action } => (*repo, *run, Some(*job), action.as_str(), WebhookEvent::WorkflowJob),
        _ => return Ok(None),
    };
    let database = &ctx.database;
    let Some(owned) = database.admin().find_repository(repo).await? else { return Ok(None) };
    let Some(run) = database.actions_runs().find_run(repo, run).await? else { return Ok(None) };
    let fork = database.forks().forks_among(&[repo]).await?.contains(&repo);
    let people = people(ctx, [run.actor, run.trigger_actor]).await?;
    let sender = run.trigger_actor.or(run.actor).and_then(|id| people.get(&id)).map_or_else(actions_bot, |who| account(who, base_url));
    let mut body = json!({
        "action": action,
        "repository": repository(&owned, base_url, fork),
        "sender": sender,
    });
    match job {
        None => {
            body["workflow_run"] = workflow_run(&run, &owned, &people, base_url);
            body["workflow"] = json!({ "path": run.workflow_path, "name": run.workflow_name });
        }
        Some(job) => {
            let store = database.actions_runs();
            let Some(job) = store.find_job(repo, job).await? else { return Ok(None) };
            let steps = store.steps_of_tasks(job.task_id.as_slice()).await?;
            body["workflow_job"] = workflow_job(&job, &run, &owned, &steps, base_url);
        }
    }
    Ok(Some(HookPayload { body: with_organization(body, &owned, base_url), repo: owned, event: kind }))
}

/// The accounts among `ids`, by id, in one query: what [`workflow_run`] takes its actors from.
pub async fn people(
    ctx: &ServiceContext,
    ids: impl IntoIterator<Item = Option<AccountId>>,
) -> Result<BTreeMap<AccountId, Account>, ServiceError> {
    let mut ids: Vec<AccountId> = ids.into_iter().flatten().collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let accounts = ctx.database.accounts().find_accounts(&ids).await?;
    Ok(accounts.into_iter().map(|account| (account.id, account)).collect())
}

/// The file name a workflow is known by in URLs and the API (`ci.yml`).
pub fn workflow_file(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// A run as `workflow_run` and the REST API show it; its actors come from `people`
/// ([`people`]), so a list needs one lookup for all its runs.
pub fn workflow_run(run: &Run, repo: &OwnedRepository, people: &BTreeMap<AccountId, Account>, base_url: &str) -> Value {
    let full_name = repo.full_name();
    let api = format!("{base_url}/api/v1/repos/{full_name}/actions");
    let person = |id: Option<AccountId>| id.and_then(|id| people.get(&id)).map(|who| account(who, base_url));
    let actor = person(run.actor);
    let triggering = person(run.trigger_actor.or(run.actor));
    let head_branch = run.reference.strip_prefix("refs/heads/").or_else(|| run.reference.strip_prefix("refs/tags/"));
    json!({
        "id": run.id.0,
        "name": run.workflow_name,
        "display_title": run.title,
        "path": run.workflow_path,
        "workflow_id": workflow_file(&run.workflow_path),
        "run_number": run.number,
        "run_attempt": run.attempt,
        "event": run.event.as_str(),
        "status": if run.is_workflow_error() { "completed" } else { github_status(run.status) },
        "conclusion": if run.is_workflow_error() { Some("failure") } else { conclusion(run.status) },
        "head_branch": head_branch,
        "head_sha": run.sha,
        "pull_requests": run.pull_number.map_or_else(Vec::new, |number| vec![json!({ "number": number })]),
        "html_url": format!("{base_url}/{full_name}/actions/runs/{}", run.number),
        "url": format!("{api}/runs/{}", run.id.0),
        "jobs_url": format!("{api}/runs/{}/jobs", run.id.0),
        "cancel_url": format!("{api}/runs/{}/cancel", run.id.0),
        "rerun_url": format!("{api}/runs/{}/rerun", run.id.0),
        "created_at": timestamp(run.created_at),
        "run_started_at": run.started_at.map(timestamp),
        "updated_at": timestamp(run.finished_at.or(run.started_at).unwrap_or(run.created_at)),
        "actor": actor,
        "triggering_actor": triggering,
        "is_schedule": run.event == RunEvent::Schedule,
    })
}

/// A job as `workflow_job` and the REST API show it, with `steps`, the steps of its
/// current task (others are ignored).
pub fn workflow_job(job: &Job, run: &Run, repo: &OwnedRepository, steps: &[Step], base_url: &str) -> Value {
    let full_name = repo.full_name();
    let api = format!("{base_url}/api/v1/repos/{full_name}/actions");
    let steps: Vec<Value> = steps
        .iter()
        .filter(|step| Some(step.task_id) == job.task_id)
        .map(|step| {
            json!({
                "name": step.name,
                "number": step.number + 1,
                "status": github_status(step.status),
                "conclusion": conclusion(step.status),
                "started_at": step.started_at.map(timestamp),
                "completed_at": step.stopped_at.map(timestamp),
            })
        })
        .collect();
    json!({
        "id": job.id.0,
        "run_id": run.id.0,
        "run_attempt": job.attempt,
        "url": format!("{api}/jobs/{}", job.id.0),
        "run_url": format!("{api}/runs/{}", run.id.0),
        "html_url": job_url(base_url, repo, run.number, job.id),
        "name": job.name,
        "workflow_name": run.workflow_name,
        "head_sha": run.sha,
        "head_branch": run.reference.strip_prefix("refs/heads/"),
        "status": github_status(job.status),
        "conclusion": conclusion(job.status),
        "labels": job.runs_on,
        "runner_id": job.runner_id.map(|runner| runner.0),
        "steps": steps,
        "created_at": timestamp(job.created_at),
        "started_at": job.started_at.map(timestamp),
        "completed_at": job.finished_at.map(timestamp),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_follow_the_status() {
        assert_eq!(RunAction::of(Status::Waiting, false), RunAction::Requested);
        assert_eq!(RunAction::of(Status::Blocked, false), RunAction::Requested);
        assert_eq!(RunAction::of(Status::Waiting, true), RunAction::InProgress);
        assert_eq!(RunAction::of(Status::Cancelled, true), RunAction::Completed);
        assert_eq!(JobAction::of(Status::Blocked), None);
        assert_eq!(JobAction::of(Status::AwaitingApproval), Some(JobAction::Waiting));
        assert_eq!(JobAction::of(Status::Skipped), Some(JobAction::Completed));
        for status in Status::ALL {
            assert_eq!(conclusion(status).is_some(), status.is_done(), "{status}");
            assert_eq!(github_status(status) == "completed", status.is_done(), "{status}");
        }
    }
}
