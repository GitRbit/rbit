//! `POST poll`: handing waiting jobs to runners over a long poll (§12.6), and assembling
//! what a runner is told to run (the `JobSpec` of §12.3).
//!
//! * A poll checks the runner against the runner cache only, then loops. When
//!   `tasks_version` moved since this runner's last empty answer, it runs the claim query
//!   once (§11.8: labels and scope matched in SQL, oldest claimable job first); on a claim
//!   it mints the task token and assembles the job specification. Otherwise it waits on a
//!   `Notify` that every move of the version wakes, until `[actions] long_poll_seconds`
//!   have passed. An idle runner therefore costs one parked future and no SQLite access.
//! * One poll per runner: a second one ends the first, which answers empty. At most
//!   `[actions] max_polls` are held open across the instance; beyond that a poll is
//!   `unavailable`.
//! * A poll carries an `Idempotency-Key`. A retried poll — its answer was lost — gets the
//!   same task with a freshly minted token (only digests are stored, so the old value
//!   cannot be handed out again). The keys of claims are remembered in memory; a retry
//!   that crossed a restart is looked up in SQLite, once per runner and process.
//! * The specification comes from the job's stored [`JobTemplate`] (JSON), the `github`
//!   context of `context.rs` (never with the token), the dispatch inputs, the results and
//!   outputs of the jobs it needs, and — for trusted runs only — the secrets of
//!   `task_secrets` and the repository's and owner's variables. A job whose template does
//!   not parse or whose specification is invalid fails at once with a readable reason, and
//!   the poll goes on looking.
//! * Checkout is by SHA from `<public URL>/<owner>/<repo>`; the cache scopes are the run's
//!   own ref then the default branch's for reading, and the run's own ref for writing —
//!   `pull/<n>` for an untrusted run (§12.5.6).
//!
//! The server side of the runner protocol (stream S) owns this file.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use rbit_actions::{
    claims::RunnerClaims,
    polling::{LongPoll, PollEnd, PollRefusal, PollSlots, admit},
    trust::{Grants, Isolation},
};
use rbit_auth::TaskTokenSecret;
use rbit_core::{
    Timestamp,
    actions::{Job, Run, RunnerId, Status, Task, aggregate},
};

use rbit_workflow::{
    protocol::PollRequest,
    secret::Secret,
    spec::{
        CacheScopes, CheckoutSource, Contexts, JOB_SPEC_VERSION, JobResult, JobSpec, JobTemplate, Needed, RunCacheScope, StepAction,
        cache_ref_scope,
    },
};
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use super::{
    ActionsService,
    runners::{CachedRunner, RunnerError},
};
use crate::ServiceError;

mod claim_bridge;
mod poll_queue;

/// Longest stored step name, in characters (the column's CHECK).
const MAX_STEP_NAME_CHARS: usize = 1024;

type Resumed = rbit_actions::claims::Resumed<JobSpec>;
type Claim = rbit_actions::claims::Claim<JobSpec>;

/// The polls held open, one per runner ([`rbit_actions::polling::PollSlots`]).
pub type Polls = PollSlots<RunnerId, PollEnded>;

/// Ends a poll when a newer one of its runner takes its place.
#[derive(Debug, Clone, Default)]
pub struct PollEnded(CancellationToken);

impl PollEnded {
    pub fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    async fn cancelled(&self) {
        self.0.cancelled().await;
    }
}

impl PollEnd for PollEnded {
    fn new() -> Self {
        Self::default()
    }

    fn end(&self) {
        self.0.cancel();
    }
}

impl ActionsService {
    /// `POST poll`: a task for `runner`, or `None` once the long poll has passed, the runner
    /// polled again, or the server is shutting down. See the module documentation.
    pub async fn poll(&self, runner: &Arc<CachedRunner>, request: &PollRequest, key: Option<&str>) -> Result<Option<JobSpec>, RunnerError> {
        admit(request, key, runner.disabled).map_err(|refusal| match refusal {
            PollRefusal::Disabled => RunnerError::forbidden(refusal.message()),
            refusal => RunnerError::bad_request(refusal.message()),
        })?;
        let Some(slot) = self.inner.polls.enter(runner.id, self.config().max_polls) else {
            return Err(RunnerError::unavailable(PollRefusal::Full.message()));
        };
        let deadline = tokio::time::Instant::now() + self.config().long_poll();
        LongPoll::wait_for_work(&poll_queue::PollSession::new(self, runner, key, deadline, &slot.ended), key).await
    }

    /// Polls held open now.
    pub fn open_polls(&self) -> usize {
        self.inner.polls.len()
    }

    /// The task a retried poll with `key` already claimed, with a fresh token, if it still
    /// runs on this runner.
    async fn resume(&self, runner: &CachedRunner, key: &str) -> Result<Resumed, RunnerError> {
        RunnerClaims::resume(&claim_bridge::ClaimSession::new(self, runner), key).await
    }

    /// Runs the claim query once for `runner`; on a claim, the task's specification. A
    /// claimed job that cannot be run fails, and without an idempotency key the query runs
    /// again.
    async fn claim_for(&self, runner: &CachedRunner, key: Option<&str>) -> Result<Claim, RunnerError> {
        RunnerClaims::claim(!runner.labels.is_empty(), || claim_bridge::ClaimSession::new(self, runner), key).await
    }

    /// Fails a claimed task that cannot run, saying why in its log, and settles its job.
    async fn fail_unrunnable(&self, task: &Task, reason: &str) -> Result<(), ServiceError> {
        tracing::warn!(task = task.id.0, job = task.job_id.0, %reason, "claimed job cannot run");
        self.write_system_line(task.id, reason).await?;
        let store = self.ctx().database.actions_runs();
        if let Some(settled) = store.finish_task(task.id, Status::Failure, Timestamp::now()).await? {
            self.settled(&settled).await?;
        }
        self.finalise_log(task.id).await
    }

    /// What a runner is told to run for `task` of `job`: `Err` with a readable reason when
    /// the job cannot be run as stored.
    async fn job_spec(&self, job: &Job, task: &Task, token: &TaskTokenSecret) -> Result<Result<JobSpec, String>, ServiceError> {
        let database = &self.ctx().database;
        let store = database.actions_runs();
        let payload = store.job_payload(job.id).await?.ok_or(ServiceError::NotFound)?;
        let template: JobTemplate = match serde_json::from_slice(&payload) {
            Ok(template) => template,
            Err(error) => {
                return Ok(Err(format!("this job's stored template cannot be read by this server ({error}); run the workflow again")));
            }
        };
        if let Err(error) = template.validate() {
            return Ok(Err(format!("this job's stored template is invalid: {error}")));
        }
        let run = store.run(job.run_id).await?.ok_or(ServiceError::NotFound)?;
        let repo = database.admin().find_repository(run.repo_id).await?.ok_or(ServiceError::NotFound)?;

        let mut github = match self.task_context(&run, job).await? {
            Value::Object(github) => github,
            _ => Map::new(),
        };
        github.retain(|key, _| !key.eq_ignore_ascii_case("token"));
        let inputs = match database.actions_runs().event(run.id).await?.and_then(|(_, inputs)| inputs) {
            Some(inputs) => serde_json::from_str::<Map<String, Value>>(&inputs).unwrap_or_default(),
            None => Map::new(),
        };
        // Actions: reusable workflows. A called job's inputs are its workflow's.
        let run_inputs = inputs.clone();
        let inputs = if template.calls.is_empty() {
            inputs
        } else {
            let (call_github, call_vars) = self.call_contexts(&run).await?;
            match self.call_inputs(&run, &template, &call_github, &call_vars, inputs).await? {
                Ok(inputs) => inputs,
                Err(reason) => return Ok(Err(reason)),
            }
        };
        let (secrets, vars) = if Grants::of_stored(run.trusted).secrets_and_variables {
            let secrets = self.task_secrets(&run).await?.into_iter().map(|(name, value)| (name, Secret::new(value))).collect();
            let vars = database
                .actions_secrets()
                .variables_for_task(run.repo_id, repo.owner.id)
                .await?
                .into_iter()
                .map(|variable| (variable.name.as_str().to_owned(), variable.value))
                .collect();
            (secrets, vars)
        } else {
            (BTreeMap::new(), BTreeMap::new())
        };
        // Actions: reusable workflows. A called job gets the secrets its calls pass on.
        let secrets: BTreeMap<String, Secret> = if template.calls.is_empty() {
            secrets
        } else {
            let plain = secrets.iter().map(|(name, value): (&String, &Secret)| (name.clone(), value.expose().to_owned())).collect();
            let (call_github, call_vars) = self.call_contexts(&run).await?;
            self.call_secrets(&run, &template, &call_github, &call_vars, run_inputs, plain)
                .await?
                .into_iter()
                .map(|(name, value)| (name, Secret::new(value)))
                .collect()
        };
        // Actions: environments. Its branch policy decides now; its values win over the
        // repository's and owner's.
        let (mut secrets, mut vars) = (secrets, vars);
        if let Some(environment) = &template.environment {
            match self.environment_values(&run, environment).await? {
                Err(reason) => return Ok(Err(reason)),
                Ok(values) => {
                    secrets.extend(values.secrets.into_iter().map(|(name, value)| (name, Secret::new(value))));
                    vars.extend(values.variables);
                }
            }
        }
        let needs = self.needs(&run, &template).await?;
        let base_url = self.ctx().config.server.base_url();
        let full_name = repo.full_name();
        let spec = JobSpec {
            version: JOB_SPEC_VERSION,
            task_id: rbit_workflow::protocol::TaskId(task.id.0),
            contexts: Contexts { github, inputs, vars, needs },
            secrets,
            token: Secret::new(token.encode()),
            checkout: CheckoutSource {
                url: format!("{base_url}/{full_name}"),
                repository: full_name,
                sha: run.sha.clone(),
                reference: run.reference.clone(),
            },
            cache: cache_scopes(&run, &repo.repo.default_branch),
            template,
            // Actions: settings API.
            actions_policy: actions_policy(&self.effective_policies(run.repo_id).await?),
        };
        Ok(spec.validate().map(|()| spec).map_err(|error| format!("this job cannot be handed to a runner: {error}")))
    }

    /// Whether a job anywhere up `template`'s needs failed (any instance), so `failure()` holds
    /// through needs skipped because of it, as on GitHub. Jobs of called workflows say no.
    pub(super) async fn failed_ancestor(&self, run: &Run, template: &JobTemplate) -> Result<bool, ServiceError> {
        if !template.calls.is_empty() || !template.need_names.is_empty() || template.needs.is_empty() {
            return Ok(false);
        }
        let jobs = self.ctx().database.actions_runs().jobs(run.id).await?;
        let (mut seen, mut todo) = (BTreeSet::new(), template.needs.iter().map(String::as_str).collect::<Vec<_>>());
        while let Some(key) = todo.pop() {
            if !seen.insert(key) {
                continue;
            }
            for job in jobs.iter().filter(|job| job.key == key) {
                if job.status == Status::Failure {
                    return Ok(true);
                }
                todo.extend(job.needs.iter().map(String::as_str));
            }
        }
        Ok(false)
    }

    /// The `needs` context: for every needed job, its result over all its matrix instances
    /// and their outputs merged.
    pub(super) async fn needs(&self, run: &Run, template: &JobTemplate) -> Result<BTreeMap<String, Needed>, ServiceError> {
        // Actions: reusable workflows. Names that stand for a job that called a workflow, or
        // the needs of a job in a called workflow.
        if !template.calls.is_empty() || !template.need_names.is_empty() {
            if template.need_names.is_empty() {
                return Ok(BTreeMap::new());
            }
            let jobs = self.run_jobs_with_outputs(run).await?;
            let (github, vars) = self.call_contexts(run).await?;
            return Ok(template
                .need_names
                .iter()
                .map(|(name, source)| (name.clone(), super::calls::need(source, &jobs, &github, &vars)))
                .collect());
        }
        if template.needs.is_empty() {
            return Ok(BTreeMap::new());
        }
        let store = self.ctx().database.actions_runs();
        let jobs = store.jobs(run.id).await?;
        let mut needs = BTreeMap::new();
        for need in &template.needs {
            let instances: Vec<&Job> = jobs.iter().filter(|job| &job.key == need).collect();
            // A failed instance fails the job even when `fail-fast` cancelled its siblings,
            // as on GitHub, in `ExprStatus::of_needs` and in `rbit-runner exec`; a run's own
            // status lets the cancellation win (`aggregate`).
            let result = if instances.iter().any(|job| job.status == Status::Failure) {
                JobResult::Failure
            } else {
                match aggregate(instances.iter().map(|job| job.status)) {
                    Status::Success => JobResult::Success,
                    Status::Skipped => JobResult::Skipped,
                    Status::Cancelled => JobResult::Cancelled,
                    _ => JobResult::Failure,
                }
            };
            let mut outputs = BTreeMap::new();
            for job in instances {
                for (key, value) in store.outputs(job.id).await? {
                    outputs.entry(key).or_insert(value);
                }
            }
            needs.insert(need.clone(), Needed { result, outputs });
        }
        Ok(needs)
    }
}

/// The cache scopes of a task of `run` (§12.5.6): it writes where [`Isolation`] puts it, and
/// [`CacheScopes::for_run`] adds the default branch's scope to what it reads.
pub fn cache_scopes(run: &Run, default_branch: &str) -> CacheScopes {
    let write = Isolation::of(run.trusted, run.pull_number).cache_write_scope(|| cache_ref_scope(&run.reference), run.id.0);
    CacheScopes::for_run(&RunCacheScope { repository_id: run.repo_id.0, write, default_branch })
}

/// The name a step is listed under: its `name`, else what it uses, else the first line of
/// its script.
fn step_name(index: usize, step: &rbit_workflow::spec::StepTemplate) -> String {
    let name = match (&step.name, &step.action) {
        (Some(name), _) if !name.trim().is_empty() => name.trim().to_owned(),
        (_, StepAction::Uses(builtin)) => format!("Run {}", builtin.uses),
        (_, StepAction::Action(action)) => format!("Run {}", action.uses),
        (_, StepAction::Run(script)) => script
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map_or_else(|| format!("Step {}", index + 1), |line| format!("Run {line}")),
    };
    super::runs::truncate_chars(&name, MAX_STEP_NAME_CHARS)
}

impl ActionsService {
    // Actions: reusable workflows.
    /// The `github` (without the token) and `vars` a call's expressions read: the run's.
    pub(super) async fn call_contexts(&self, run: &Run) -> Result<(Map<String, Value>, Map<String, Value>), ServiceError> {
        let store = self.ctx().database.actions_runs();
        let github = match store.jobs(run.id).await?.first() {
            Some(job) => match self.task_context(run, job).await? {
                Value::Object(mut github) => {
                    github.retain(|key, _| !key.eq_ignore_ascii_case("token"));
                    github
                }
                _ => Map::new(),
            },
            None => Map::new(),
        };
        let vars = self.task_variables(run).await?.into_iter().map(|(name, value)| (name, Value::String(value))).collect();
        Ok((github, vars))
    }
}

// Actions: settings API.
/// The spec's form of a repository's effective `allowed_actions`.
fn actions_policy(policies: &rbit_core::actions::permissions::EffectivePolicies) -> rbit_workflow::spec::ActionsPolicy {
    use rbit_core::actions::permissions::AllowedActions;
    use rbit_workflow::spec::{ActionsPolicy, ActionsRule};
    let rules = policies
        .actions
        .iter()
        .filter_map(|(allowed, selected)| match allowed {
            AllowedActions::All => None,
            AllowedActions::LocalOnly => Some(ActionsRule::LocalOnly),
            AllowedActions::Selected => {
                Some(ActionsRule::Selected { github_owned: selected.github_owned_allowed, patterns: selected.patterns_allowed.clone() })
            }
        })
        .collect();
    ActionsPolicy { rules }
}

#[cfg(test)]
mod tests {
    use rbit_core::{
        RepoId,
        actions::{RunEvent, RunId},
    };

    use super::*;

    fn run(reference: &str, trusted: bool, pull: Option<i64>) -> Run {
        Run {
            id: RunId(9),
            repo_id: RepoId(4),
            number: 1,
            workflow_path: ".rbit/workflows/ci.yml".into(),
            workflow_name: "CI".into(),
            title: "t".into(),
            event: if pull.is_some() { RunEvent::PullRequest } else { RunEvent::Push },
            reference: reference.into(),
            sha: "0".repeat(40),
            actor: None,
            trigger_actor: None,
            pull_number: pull,
            is_fork: false,
            trusted,
            status: Status::Running,
            attempt: 1,
            concurrency_group: None,
            cancel_in_progress: false,
            error: None,
            approved_by: None,
            approved_at: None,
            created_at: Timestamp(1),
            started_at: None,
            finished_at: None,
            logs_expired_at: None,
            awaiting_review: false,
        }
    }

    #[test]
    fn cache_scopes_never_let_untrusted_runs_write_where_others_read() {
        let push = cache_scopes(&run("refs/heads/feature", true, None), "main");
        assert_eq!(
            (push.read.clone(), push.write.as_str(), push.repository_id),
            (vec!["heads/feature".to_owned(), "heads/main".to_owned()], "heads/feature", 4)
        );
        let main = cache_scopes(&run("refs/heads/main", true, None), "main");
        assert_eq!(main.read.clone(), ["heads/main"]);
        let trusted_pull = cache_scopes(&run("refs/pull/3/head", true, Some(3)), "main");
        assert_eq!(
            (trusted_pull.read.clone(), trusted_pull.write.as_str()),
            (vec!["pull/3/head".to_owned(), "heads/main".to_owned()], "pull/3/head")
        );
        let untrusted = cache_scopes(&run("refs/pull/3/head", false, Some(3)), "main");
        assert_eq!((untrusted.read.clone(), untrusted.write.as_str()), (vec!["pull/3".to_owned(), "heads/main".to_owned()], "pull/3"));
        assert_eq!(cache_scopes(&run("refs/heads/x", false, None), "main").write, "untrusted/9");
        for scopes in [push, main, trusted_pull, untrusted] {
            scopes.validate().unwrap();
        }
        assert!(rbit_workflow::spec::cache_ref_scope(&format!("refs/heads/{}", "x".repeat(300))).starts_with("ref/"));
    }

    #[test]
    fn polls_are_one_per_runner_and_bounded() {
        let polls = Polls::default();
        let first = polls.enter(RunnerId(1), 2).unwrap();
        let second = polls.enter(RunnerId(1), 2).unwrap();
        assert!(first.ended.is_cancelled(), "a second poll ends the first");
        assert!(!second.ended.is_cancelled());
        drop(first);
        assert_eq!(polls.len(), 1, "the ended poll does not free the newer one's place");
        let _other = polls.enter(RunnerId(2), 2).unwrap();
        assert!(polls.enter(RunnerId(3), 2).is_none(), "the instance holds at most max polls");
        drop(second);
        assert!(polls.enter(RunnerId(3), 2).is_some());
    }
}
