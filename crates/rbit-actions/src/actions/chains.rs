//! Runs that other runs and the REST API start: `workflow_run` (a run of the same
//! repository was requested, started or completed) and `repository_dispatch`
//! (`POST /repos/{owner}/{repo}/dispatches`).
//!
//! * Both run the workflows of the default branch at its newest commit, as on GitHub, and
//!   are trusted like a push to it: the code that runs is the default branch's, which only
//!   writers change. A `workflow_run` run therefore gets secrets and the repository's token
//!   permission even when the run that started it had neither (a fork's pull request), as
//!   on GitHub; its workflow must treat that run's artifacts and outputs as untrusted input.
//! * `workflow_run` matches the triggering workflow's `name:`, the action (`types`, all three
//!   when absent) and the triggering run's branch (`branches`, `branches-ignore`). Chains
//!   stop after [`MAX_WORKFLOW_RUN_CHAIN`] levels, GitHub's limit: a run that a chain of
//!   three `workflow_run`s started starts none, which also ends a workflow that triggers on
//!   itself. Workflow files that do not parse are passed over: they cannot say what they
//!   trigger on.
//! * `repository_dispatch` needs write access. `event_type` is 1 to 100 characters and
//!   `client_payload` an object of at most 10 top-level properties and 65,535 characters
//!   of JSON, GitHub's limits. Workflows listing `types` run only for those event types.
//!
//! Evaluation (Git reads and YAML) runs on the bounded evaluation semaphore, as for pushes.

use rbit_core::{
    RepoId, User,
    actions::{Run, RunEvent, RunId, RunTrust},
    repository::OwnedRepository,
};
use rbit_workflow::{Event, GroupContext, model::WorkflowRunType};
use serde_json::{Map, Value, json};

use super::{
    ActionsService, context,
    hooks::{self, RunAction},
    runs::{Candidate, Trigger},
    triggers::{discover, plan_workflow},
};
use crate::{
    RepoContext, ServiceError,
    webhooks::payloads::{account, repository, with_organization},
};

/// How many `workflow_run` levels may follow one another.
pub const MAX_WORKFLOW_RUN_CHAIN: usize = 3;
/// The longest `event_type` of a `repository_dispatch`.
pub const MAX_DISPATCH_EVENT_TYPE_CHARS: usize = 100;
/// The most top-level properties of a `client_payload`.
pub const MAX_CLIENT_PAYLOAD_PROPERTIES: usize = 10;
/// The longest `client_payload`, as JSON.
pub const MAX_CLIENT_PAYLOAD_CHARS: usize = 65_535;

impl ActionsService {
    /// Starts the `workflow_run` runs that `action` of run `id` asks for and returns them.
    /// Public so tests can drive it without the subscriber.
    pub async fn handle_workflow_run(&self, repo: RepoId, id: RunId, action: RunAction) -> Result<Vec<Run>, ServiceError> {
        let database = &self.ctx().database;
        if !database.actions_settings().is_enabled(repo).await? {
            return Ok(Vec::new());
        }
        let Some(run) = database.actions_runs().run(id).await? else { return Ok(Vec::new()) };
        if self.chain_depth(&run).await? >= MAX_WORKFLOW_RUN_CHAIN {
            tracing::debug!(run = run.id.0, "a workflow_run chain is {MAX_WORKFLOW_RUN_CHAIN} levels deep; it starts no more runs");
            return Ok(Vec::new());
        }
        let Some(owned) = database.admin().find_repository(repo).await? else { return Ok(Vec::new()) };
        let kind = match action {
            RunAction::Requested => WorkflowRunType::Requested,
            RunAction::InProgress => WorkflowRunType::InProgress,
            RunAction::Completed => WorkflowRunType::Completed,
        };
        let branch = run.reference.strip_prefix("refs/heads/").or_else(|| run.reference.strip_prefix("refs/tags/")).map(str::to_owned);
        let (name, actors) = (run.workflow_name.clone(), (run.actor, run.trigger_actor));
        let reference = format!("refs/heads/{}", owned.repo.default_branch);
        let reader = self.reader(&owned);
        let Ok(sha) = reader.commit_id(reference.clone()).await else { return Ok(Vec::new()) };
        let group = self.planning(&owned, RunEvent::WorkflowRun, &reference, &sha, actors, true).await?;
        let candidates = self
            .default_branch_candidates(&owned, &sha, group, move |workflow| {
                rbit_workflow::matches(
                    workflow,
                    &Event::WorkflowRun { workflow: &name, action: kind, branch: branch.as_deref() },
                    &mut None::<Vec<String>>,
                )
            })
            .await?;
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let people = self.people([run.actor, run.trigger_actor]).await?;
        let base_url = self.ctx().config.server.base_url();
        let fork = database.forks().forks_among(&[repo]).await?.contains(&repo);
        let sender = run.trigger_actor.or(run.actor).and_then(|id| people.get(&id));
        let mut body = json!({
            "action": action.as_str(),
            "workflow_run": hooks::workflow_run(&run, &owned, &people, base_url),
            "workflow": {
                "name": run.workflow_name,
                "path": run.workflow_path,
                "html_url": format!("{base_url}/{}/actions/workflows/{}", owned.full_name(), workflow_file_name(&run.workflow_path)),
            },
            "repository": repository(&owned, base_url, fork),
        });
        if let Some(sender) = sender {
            body["sender"] = account(sender, base_url);
        }
        let payload = context::fit_payload(with_organization(body, &owned, base_url));
        let trigger = Trigger {
            repo: &owned,
            event: RunEvent::WorkflowRun,
            reference: &reference,
            sha: &sha,
            title: &run.title,
            actor: run.actor,
            trigger_actor: run.trigger_actor,
            pull_number: None,
            is_fork: false,
            trust: RunTrust::Trusted,
            payload: &payload,
            inputs: None,
        };
        let mut allowance = self.allowance();
        let mut started = Vec::new();
        for candidate in candidates {
            started.extend(self.start_run(&trigger, candidate, &mut allowance).await?);
        }
        Ok(started)
    }

    /// How many `workflow_run` runs led to `run`: 0 for a run something else started.
    async fn chain_depth(&self, run: &Run) -> Result<usize, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let (mut depth, mut current) = (0, run.clone());
        while current.event == RunEvent::WorkflowRun && depth < MAX_WORKFLOW_RUN_CHAIN {
            depth += 1;
            let Some((payload, _)) = store.event(current.id).await? else { break };
            let parent = serde_json::from_str::<Value>(&payload).ok().and_then(|payload| payload["workflow_run"]["id"].as_i64());
            let Some(next) = (match parent {
                Some(parent) => store.run(RunId(parent)).await?,
                None => None,
            }) else {
                break;
            };
            current = next;
        }
        Ok(depth)
    }

    /// `POST /repos/{owner}/{repo}/dispatches` as `actor`, who needs write access: starts
    /// the default branch's workflows that run on `repository_dispatch` of `event_type`.
    pub async fn repository_dispatch(
        &self,
        actor: &User,
        repo: &RepoContext,
        event_type: &str,
        client_payload: Option<Value>,
    ) -> Result<Vec<Run>, ServiceError> {
        if !repo.access.can_write() {
            return Err(if repo.access.can_read() { ServiceError::Forbidden } else { ServiceError::NotFound });
        }
        repo.ensure_writable()?;
        let event_type = event_type.trim();
        if event_type.is_empty() || event_type.chars().count() > MAX_DISPATCH_EVENT_TYPE_CHARS {
            return Err(ServiceError::Invalid(format!("event_type must be 1 to {MAX_DISPATCH_EVENT_TYPE_CHARS} characters")));
        }
        let client_payload = match client_payload {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(object)) => object,
            Some(_) => return Err(ServiceError::Invalid("client_payload must be an object".to_owned())),
        };
        if client_payload.len() > MAX_CLIENT_PAYLOAD_PROPERTIES {
            return Err(ServiceError::Invalid(format!(
                "client_payload may have at most {MAX_CLIENT_PAYLOAD_PROPERTIES} top-level properties"
            )));
        }
        if Value::Object(client_payload.clone()).to_string().chars().count() > MAX_CLIENT_PAYLOAD_CHARS {
            return Err(ServiceError::Invalid(format!("client_payload may be at most {MAX_CLIENT_PAYLOAD_CHARS} characters")));
        }
        let owned = &repo.repo;
        if !self.ctx().database.actions_settings().is_enabled(owned.repo.id).await? {
            return Ok(Vec::new());
        }
        let reference = format!("refs/heads/{}", owned.repo.default_branch);
        let Ok(sha) = repo.reader.commit_id(reference.clone()).await else { return Ok(Vec::new()) };
        let group =
            self.planning(owned, RunEvent::RepositoryDispatch, &reference, &sha, (Some(actor.id()), Some(actor.id())), true).await?;
        let wanted = event_type.to_owned();
        let candidates = self
            .default_branch_candidates(owned, &sha, group, move |workflow| {
                rbit_workflow::matches(workflow, &Event::RepositoryDispatch { event_type: &wanted }, &mut None::<Vec<String>>)
            })
            .await?;
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let base_url = self.ctx().config.server.base_url();
        let fork = self.ctx().database.forks().forks_among(&[owned.repo.id]).await?.contains(&owned.repo.id);
        let body = json!({
            "action": event_type,
            "branch": owned.repo.default_branch,
            "client_payload": client_payload,
            "repository": repository(owned, base_url, fork),
            "sender": account(&actor.account, base_url),
        });
        let payload = context::fit_payload(with_organization(body, owned, base_url));
        let trigger = Trigger {
            repo: owned,
            event: RunEvent::RepositoryDispatch,
            reference: &reference,
            sha: &sha,
            title: event_type,
            actor: Some(actor.id()),
            trigger_actor: Some(actor.id()),
            pull_number: None,
            is_fork: false,
            trust: RunTrust::Trusted,
            payload: &payload,
            inputs: None,
        };
        let mut allowance = self.allowance();
        let mut started = Vec::new();
        for candidate in candidates {
            started.extend(self.start_run(&trigger, candidate, &mut allowance).await?);
        }
        tracing::info!(repo = %owned.full_name(), event_type, runs = started.len(), by = %actor.name(), "repository dispatched");
        Ok(started)
    }

    /// The default branch's workflows at `sha` that `wanted` accepts, planned with `group`.
    /// Files that do not parse are passed over.
    async fn default_branch_candidates(
        &self,
        owned: &OwnedRepository,
        sha: &str,
        group: GroupContext,
        wanted: impl Fn(&rbit_workflow::Workflow) -> bool + Send + 'static,
    ) -> Result<Vec<Candidate>, ServiceError> {
        let (reader, limits, at) = (self.reader(owned), self.limits(), sha.to_owned());
        let (this, caller) = (self.clone(), owned.clone());
        self.evaluate(move |handle, deadline| {
            let files = match discover(handle, &reader, &at, &limits, deadline) {
                Ok(files) => files,
                Err(error) => {
                    tracing::warn!(%error, "cannot read the default branch's workflows");
                    return Vec::new();
                }
            };
            files
                .iter()
                .filter_map(|file| {
                    let workflow = file.parsed.as_ref().ok().filter(|workflow| wanted(workflow))?;
                    let mut loader = super::calls::CallLoader::new(handle, &this, &caller, &reader, &at, &limits, deadline);
                    let outcome = plan_workflow(workflow, &file.name(), &file.path, &group, &limits, deadline, &mut loader)
                        .map_err(|error| super::triggers::problem(&file.path, &error));
                    Some(Candidate { path: file.path.clone(), name: file.name(), outcome })
                })
                .collect()
        })
        .await
    }
}

/// The file name of a workflow path, `ci.yml` of `.rbit/workflows/ci.yml`.
fn workflow_file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
