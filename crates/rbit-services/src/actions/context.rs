//! The `Task.context` document a runner receives and the GitHub-shaped `event` payloads of
//! push, pull request, dispatch and schedule runs.
//!
//! A run stores one thing about its trigger: the event payload a workflow sees as
//! `github.event` (`actions_runs.event_payload`, at most [`MAX_EVENT_PAYLOAD_BYTES`]), plus
//! the checked inputs of a dispatch (`dispatch_inputs`, typed). Push and pull request
//! payloads are the webhook payloads (`webhooks::payloads::PushPayload`,
//! `pulls::hooks::payload`), so a workflow reads the same fields a webhook receiver does.
//! Everything else in the `github` context follows from the run row:
//! [`ActionsService::task_context`] assembles it when a task is handed out, and the
//! scheduler adds the task's `token`.
//!
//! Workstream T (triggers and runs) owns this file.

use std::collections::BTreeMap;

use rbit_core::{
    Account, AccountId,
    actions::{Job, Run, RunEvent},
    repository::OwnedRepository,
};
use rbit_git::{CommitSummary, RefUpdate};
use rbit_workflow::{GroupContext, trigger::InputValue};
use serde_json::{Map, Value, json};

use super::ActionsService;
use crate::{
    ServiceError,
    webhooks::payloads::{PushPayload, account, repository, with_organization},
};

/// Largest stored event payload, in bytes (the `actions_runs.event_payload` CHECK).
pub const MAX_EVENT_PAYLOAD_BYTES: usize = 256 * 1024;

/// The `push` event of one updated ref: the `push` webhook payload.
pub fn push_event(
    repo: &OwnedRepository,
    update: &RefUpdate,
    commits: &[CommitSummary],
    pusher: Option<&Account>,
    fork: bool,
    base_url: &str,
) -> Value {
    PushPayload { repo, update, commits, total: commits.len(), pusher, fork, base_url }.to_json()
}

/// The `workflow_dispatch` event: GitHub's `inputs` (every value a string), `ref`,
/// `workflow`, `repository` and `sender`.
pub fn dispatch_event(
    repo: &OwnedRepository,
    fork: bool,
    sender: &Account,
    workflow_path: &str,
    reference: &str,
    inputs: &BTreeMap<String, InputValue>,
    base_url: &str,
) -> Value {
    let inputs: Map<String, Value> = inputs.iter().map(|(name, value)| (name.clone(), Value::String(value.to_text()))).collect();
    let body = json!({
        "inputs": inputs,
        "ref": reference,
        "workflow": workflow_path,
        "repository": repository(repo, base_url, fork),
        "sender": account(sender, base_url),
    });
    with_organization(body, repo, base_url)
}

/// The `schedule` event: the expression that fired and the repository. Nobody sent it.
pub fn schedule_event(repo: &OwnedRepository, fork: bool, cron: &str, base_url: &str) -> Value {
    with_organization(json!({ "schedule": cron, "repository": repository(repo, base_url, fork) }), repo, base_url)
}

/// The checked dispatch inputs with their types, as the `inputs` context has them.
pub fn typed_inputs(inputs: &BTreeMap<String, InputValue>) -> Value {
    let typed: Map<String, Value> = inputs
        .iter()
        .map(|(name, value)| {
            let value = match value {
                InputValue::String(text) => Value::String(text.clone()),
                InputValue::Boolean(flag) => Value::Bool(*flag),
                InputValue::Number(number) => serde_json::Number::from_f64(*number).map_or(Value::Null, Value::Number),
            };
            (name.clone(), value)
        })
        .collect();
    Value::Object(typed)
}

/// `payload` as stored: within [`MAX_EVENT_PAYLOAD_BYTES`]. The bulky parts go first — the
/// commit list, then a pull request's body — and as a last resort only the small fields
/// that identify the event are kept.
pub fn fit_payload(mut payload: Value) -> String {
    let text = payload.to_string();
    if text.len() <= MAX_EVENT_PAYLOAD_BYTES {
        return text;
    }
    if let Some(commits) = payload.get_mut("commits") {
        *commits = Value::Array(Vec::new());
    }
    if let Some(body) = payload.get_mut("pull_request").and_then(|pull| pull.get_mut("body")) {
        *body = Value::Null;
    }
    let text = payload.to_string();
    if text.len() <= MAX_EVENT_PAYLOAD_BYTES {
        return text;
    }
    let keep = ["action", "number", "ref", "before", "after", "inputs", "schedule", "workflow"];
    let reduced: Map<String, Value> = keep
        .iter()
        .filter_map(|key| payload.get(*key).map(|value| ((*key).to_owned(), value.clone())))
        .filter(|(_, value)| value.to_string().len() <= MAX_EVENT_PAYLOAD_BYTES / 16)
        .collect();
    Value::Object(reduced).to_string()
}

/// `github.ref_name`: the branch or tag name, or `<n>/head` for a pull request.
pub fn ref_name(reference: &str) -> &str {
    ["refs/heads/", "refs/tags/", "refs/pull/"].iter().find_map(|prefix| reference.strip_prefix(prefix)).unwrap_or(reference)
}

/// `github.ref_type`: `tag` for tags, `branch` for everything else (a pull request runs on
/// a branch-like ref).
pub fn ref_type(reference: &str) -> &'static str {
    if reference.starts_with("refs/tags/") { "tag" } else { "branch" }
}

/// Everything [`github_context`] needs about a run, already loaded.
#[derive(Debug, Clone)]
pub struct ContextInput<'a> {
    pub run: &'a Run,
    /// The job's key under `jobs`.
    pub job: &'a str,
    pub repo: &'a OwnedRepository,
    /// The stored event payload.
    pub event: Value,
    /// The stored dispatch inputs (typed), if any.
    pub inputs: Option<Value>,
    /// Logins of the run's actor and triggering actor (empty when there is none).
    pub actor: &'a str,
    pub triggering_actor: &'a str,
    pub base_url: &'a str,
    pub retention_days: u32,
}

/// The `github` context of a task, without its `token`. Ids and numbers are strings, as
/// GitHub's runner environment has them.
pub fn github_context(input: &ContextInput<'_>) -> Value {
    let run = input.run;
    let pull = (run.event == RunEvent::PullRequest).then(|| &input.event["pull_request"]);
    let branch_of = |side: &str| pull.and_then(|pull| pull[side]["ref"].as_str()).unwrap_or_default().to_owned();
    let full_name = input.repo.full_name();
    let mut context = json!({
        "event": input.event,
        "event_name": run.event.as_str(),
        "repository": full_name,
        "repository_owner": input.repo.owner.name.as_str(),
        "repository_id": input.repo.repo.id.0.to_string(),
        "sha": run.sha,
        "ref": run.reference,
        "ref_name": ref_name(&run.reference),
        "ref_type": ref_type(&run.reference),
        "head_ref": branch_of("head"),
        "base_ref": branch_of("base"),
        "actor": input.actor,
        "triggering_actor": input.triggering_actor,
        "workflow": run.workflow_name,
        "workflow_ref": format!("{full_name}/{}@{}", run.workflow_path, run.reference),
        "job": input.job,
        "run_id": run.id.0.to_string(),
        "run_number": run.number.to_string(),
        "run_attempt": run.attempt.to_string(),
        "server_url": input.base_url,
        "api_url": format!("{}/api/v1", input.base_url),
        "retention_days": input.retention_days.to_string(),
    });
    if let Some(inputs) = &input.inputs {
        context["inputs"] = inputs.clone();
    }
    context
}

impl ActionsService {
    // Actions: join (J).
    /// What planning a run of `repo` knows before the run exists (see
    /// [`rbit_workflow::GroupContext`]): the repository, the event, the ref and commit, the
    /// logins of who caused and who triggered it (`actors`), and — for a trusted run only — the
    /// repository's then the owner's variables. Callers add what only their event has.
    pub(super) async fn planning(
        &self,
        repo: &OwnedRepository,
        event: RunEvent,
        reference: &str,
        sha: &str,
        (actor, triggering_actor): (Option<AccountId>, Option<AccountId>),
        trusted: bool,
    ) -> Result<GroupContext, ServiceError> {
        let database = &self.ctx().database;
        let ids: Vec<AccountId> = actor.into_iter().chain(triggering_actor).collect();
        let accounts = database.accounts().find_accounts(&ids).await?;
        let login = |id: Option<AccountId>| {
            id.and_then(|id| accounts.iter().find(|account| account.id == id)).map(|account| account.name.to_string()).unwrap_or_default()
        };
        let mut vars = BTreeMap::new();
        if trusted {
            for variable in database.actions_secrets().variables_for_task(repo.repo.id, repo.owner.id).await? {
                vars.entry(variable.name.to_string()).or_insert(variable.value);
            }
        }
        Ok(GroupContext {
            reference: reference.to_owned(),
            sha: sha.to_owned(),
            event_name: event.as_str().to_owned(),
            repository: repo.full_name(),
            repository_owner: repo.owner.name.to_string(),
            repository_id: repo.repo.id.0,
            actor: login(actor),
            triggering_actor: login(triggering_actor.or(actor)),
            server_url: self.ctx().config.server.base_url().to_owned(),
            vars,
            ..GroupContext::default()
        })
    }

    /// The `github` context of a task of `job` in `run`, without the `token` the scheduler
    /// adds. Logins are read now: an actor whose account is gone is an empty string.
    pub async fn task_context(&self, run: &Run, job: &Job) -> Result<Value, ServiceError> {
        let database = &self.ctx().database;
        let repo = database.admin().find_repository(run.repo_id).await?.ok_or(ServiceError::NotFound)?;
        let (event, inputs) = database.actions_runs().event(run.id).await?.ok_or(ServiceError::NotFound)?;
        let event = serde_json::from_str(&event).unwrap_or(Value::Null);
        let inputs = inputs.and_then(|inputs| serde_json::from_str(&inputs).ok());
        let ids: Vec<AccountId> = run.actor.into_iter().chain(run.trigger_actor).collect();
        let accounts = database.accounts().find_accounts(&ids).await?;
        let login = |id: Option<AccountId>| {
            id.and_then(|id| accounts.iter().find(|account| account.id == id)).map(|account| account.name.to_string()).unwrap_or_default()
        };
        let (actor, triggering_actor) = (login(run.actor), login(run.trigger_actor));
        Ok(github_context(&ContextInput {
            run,
            job: &job.key,
            repo: &repo,
            event,
            inputs,
            actor: &actor,
            triggering_actor: &triggering_actor,
            base_url: self.ctx().config.server.base_url(),
            retention_days: self.config().log_retention_days,
        }))
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{
        AccountKind, RepoId, Timestamp, Visibility,
        actions::{RunId, Status},
        repository::Repository,
    };

    use super::*;

    fn repo() -> OwnedRepository {
        let owner = Account {
            id: AccountId(1),
            kind: AccountKind::User,
            name: "alice".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(0),
        };
        OwnedRepository {
            repo: Repository {
                id: RepoId(9),
                owner_id: owner.id,
                name: "rocket".parse().unwrap(),
                description: None,
                visibility: Visibility::Public,
                default_branch: "main".into(),
                created_at: Timestamp(0),
                updated_at: Timestamp(0),
                pushed_at: None,
                archived_at: None,
            },
            owner,
        }
    }

    fn run(event: RunEvent, reference: &str) -> Run {
        Run {
            id: RunId(41),
            repo_id: RepoId(9),
            number: 7,
            workflow_path: ".forgejo/workflows/ci.yml".into(),
            workflow_name: "CI".into(),
            title: "Fix".into(),
            event,
            reference: reference.into(),
            sha: "a".repeat(40),
            actor: Some(AccountId(1)),
            trigger_actor: Some(AccountId(1)),
            pull_number: None,
            is_fork: false,
            trusted: true,
            status: Status::Waiting,
            attempt: 2,
            concurrency_group: None,
            cancel_in_progress: false,
            error: None,
            approved_by: None,
            approved_at: None,
            created_at: Timestamp(0),
            started_at: None,
            finished_at: None,
            logs_expired_at: None,
        }
    }

    fn github(run: &Run, event: Value, inputs: Option<Value>) -> Value {
        let repo = repo();
        github_context(&ContextInput {
            run,
            job: "build",
            repo: &repo,
            event,
            inputs,
            actor: "alice",
            triggering_actor: "bob",
            base_url: "https://git.example.com",
            retention_days: 90,
        })
    }

    #[test]
    fn refs_have_names_and_types() {
        assert_eq!((ref_name("refs/heads/feature/x"), ref_type("refs/heads/feature/x")), ("feature/x", "branch"));
        assert_eq!((ref_name("refs/tags/v1.0"), ref_type("refs/tags/v1.0")), ("v1.0", "tag"));
        assert_eq!((ref_name("refs/pull/7/head"), ref_type("refs/pull/7/head")), ("7/head", "branch"));
        assert_eq!(ref_name("HEAD"), "HEAD");
    }

    #[test]
    fn a_push_context_has_every_key_runners_read() {
        let run = run(RunEvent::Push, "refs/heads/main");
        let context = github(&run, json!({ "ref": "refs/heads/main" }), None);
        let expected = json!({
            "event": { "ref": "refs/heads/main" },
            "event_name": "push",
            "repository": "alice/rocket",
            "repository_owner": "alice",
            "repository_id": "9",
            "sha": "a".repeat(40),
            "ref": "refs/heads/main",
            "ref_name": "main",
            "ref_type": "branch",
            "head_ref": "",
            "base_ref": "",
            "actor": "alice",
            "triggering_actor": "bob",
            "workflow": "CI",
            "workflow_ref": "alice/rocket/.forgejo/workflows/ci.yml@refs/heads/main",
            "job": "build",
            "run_id": "41",
            "run_number": "7",
            "run_attempt": "2",
            "server_url": "https://git.example.com",
            "api_url": "https://git.example.com/api/v1",
            "retention_days": "90",
        });
        assert_eq!(context, expected);
    }

    #[test]
    fn pull_request_and_dispatch_contexts_add_their_refs_and_inputs() {
        let pull = run(RunEvent::PullRequest, "refs/pull/3/head");
        let event = json!({ "pull_request": { "head": { "ref": "feature" }, "base": { "ref": "main" } } });
        let context = github(&pull, event, None);
        assert_eq!((context["head_ref"].as_str(), context["base_ref"].as_str()), (Some("feature"), Some("main")));
        assert_eq!(context["ref_name"], "3/head");

        let dispatch = run(RunEvent::WorkflowDispatch, "refs/tags/v1");
        let inputs = BTreeMap::from([
            ("debug".to_owned(), InputValue::Boolean(true)),
            ("level".to_owned(), InputValue::Number(2.5)),
            ("name".to_owned(), InputValue::String("x".into())),
        ]);
        let event = dispatch_event(&repo(), false, &repo().owner, ".forgejo/workflows/ci.yml", "refs/tags/v1", &inputs, "https://g");
        assert_eq!(event["inputs"], json!({ "debug": "true", "level": "2.5", "name": "x" }), "GitHub's event inputs are strings");
        assert_eq!(event["sender"]["login"], "alice");
        let context = github(&dispatch, event, Some(typed_inputs(&inputs)));
        assert_eq!(context["inputs"], json!({ "debug": true, "level": 2.5, "name": "x" }));
        assert_eq!((context["ref_type"].as_str(), context["head_ref"].as_str()), (Some("tag"), Some("")));
    }

    #[test]
    fn oversized_payloads_lose_their_bulk_first() {
        let small = json!({ "ref": "refs/heads/main", "commits": [1, 2] });
        assert_eq!(fit_payload(small.clone()), small.to_string());
        let big = "x".repeat(MAX_EVENT_PAYLOAD_BYTES);
        let commits = json!({ "ref": "refs/heads/main", "commits": [big] });
        assert_eq!(fit_payload(commits), json!({ "ref": "refs/heads/main", "commits": [] }).to_string());
        let pull = json!({ "action": "opened", "number": 3, "pull_request": { "body": big, "title": "t" } });
        let fitted = json!({ "action": "opened", "number": 3, "pull_request": { "body": null, "title": "t" } });
        assert_eq!(fit_payload(pull), fitted.to_string());
        let hopeless = json!({ "action": "opened", "number": 3, "pull_request": { "title": big } });
        assert_eq!(fit_payload(hopeless), json!({ "action": "opened", "number": 3 }).to_string());
        let schedule = schedule_event(&repo(), false, "0 * * * *", "https://g");
        assert_eq!(
            (schedule["schedule"].as_str(), schedule["repository"]["full_name"].as_str()),
            (Some("0 * * * *"), Some("alice/rocket"))
        );
    }
}
