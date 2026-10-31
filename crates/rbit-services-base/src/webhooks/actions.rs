//! Actions webhook and REST representations shared with the optional control plane.

use crate::{
    DomainEvent, ServiceContext, ServiceError,
    statuses::hooks::actions_bot,
    webhooks::{
        HookPayload,
        payloads::{account, repository, timestamp, with_organization},
    },
};
use rbit_core::{
    Account, AccountId,
    actions::{Job, JobId, Run, RunEvent, Status, Step},
    repository::OwnedRepository,
    webhook::WebhookEvent,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The job page linked by webhook and status representations.
pub fn job_url(base_url: &str, repo: &OwnedRepository, run_number: i64, job: JobId) -> String {
    format!("{base_url}/{}/actions/runs/{run_number}/jobs/{}", repo.full_name(), job.0)
}

/// GitHub's `status` field: `queued`, `waiting`, `in_progress` or `completed`.
pub const fn github_status(status: Status) -> &'static str {
    match status {
        Status::Waiting | Status::Blocked => "queued",
        Status::AwaitingApproval => "waiting",
        Status::Running => "in_progress",
        Status::Success | Status::Failure | Status::Cancelled | Status::Skipped => "completed",
    }
}

/// GitHub's `conclusion` field, `null` until finished.
pub const fn conclusion(status: Status) -> Option<&'static str> {
    match status {
        Status::Success => Some("success"),
        Status::Failure => Some("failure"),
        Status::Cancelled => Some("cancelled"),
        Status::Skipped => Some("skipped"),
        Status::Waiting | Status::Running | Status::Blocked | Status::AwaitingApproval => None,
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
            let runner = match job.runner_id {
                Some(id) => database.actions_runners().find(id).await?,
                None => None,
            };
            body["workflow_job"] = workflow_job(&job, &run, &owned, &steps, runner.as_ref().map(|runner| runner.name.as_str()), base_url);
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
        // Actions: environments. A run whose deployments wait for review is `waiting`.
        "status": if run.is_workflow_error() {
            "completed"
        } else if run.awaiting_review {
            "waiting"
        } else {
            github_status(run.status)
        },
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
/// `runner_name` is the name of the runner that took it (`job.runner_id`), if known.
pub fn workflow_job(job: &Job, run: &Run, repo: &OwnedRepository, steps: &[Step], runner_name: Option<&str>, base_url: &str) -> Value {
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
        "runner_name": runner_name,
        "steps": steps,
        "created_at": timestamp(job.created_at),
        "started_at": job.started_at.map(timestamp),
        "completed_at": job.finished_at.map(timestamp),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbit_core::actions::{JobAction, RunAction};

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
