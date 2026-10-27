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
    collections::{BTreeMap, HashMap},
    pin::pin,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use rbit_auth::TaskTokenSecret;
use rbit_core::{
    Timestamp,
    actions::{Job, Run, RunnerId, Status, Task, aggregate},
};
use rbit_storage::sqlite::actions_runs::ClaimRequest;
use rbit_workflow::{
    protocol::{PollRequest, valid_idempotency_key},
    secret::Secret,
    spec::{CacheScopes, CheckoutSource, Contexts, JOB_SPEC_VERSION, JobResult, JobSpec, JobTemplate, Needed, StepAction},
};
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use super::{
    ActionsService,
    runners::{CachedRunner, RunnerError},
};
use crate::ServiceError;

/// Longest stored step name, in characters (the column's CHECK).
const MAX_STEP_NAME_CHARS: usize = 1024;

/// What a retried poll's key found.
enum Resumed {
    /// Its task, still running, with a new token.
    Task(Box<JobSpec>),
    /// A task that has stopped since (or a token already cleared by a cancel).
    Spent,
    /// Nothing: the key is new.
    Unknown,
}

/// What one run of the claim query gave a runner.
enum Claim {
    Task(Box<JobSpec>),
    Nothing,
    /// A job was claimed with the poll's idempotency key but could not run.
    Spent,
}

/// The polls held open, one per runner, and how many there are.
#[derive(Debug, Default)]
pub struct Polls {
    by_runner: Mutex<HashMap<RunnerId, (u64, CancellationToken)>>,
    next: AtomicU64,
}

/// A poll's place in [`Polls`]; dropping it frees the place.
#[derive(Debug)]
struct PollSlot<'a> {
    polls: &'a Polls,
    runner: RunnerId,
    id: u64,
    ended: CancellationToken,
}

impl Polls {
    /// Takes a place for `runner`'s poll, ending the poll it may already have. `None` when
    /// `max` polls are already held open by other runners.
    fn enter(&self, runner: RunnerId, max: usize) -> Option<PollSlot<'_>> {
        let mut polls = self.by_runner.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((_, previous)) = polls.get(&runner) {
            previous.cancel();
        } else if polls.len() >= max {
            return None;
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let ended = CancellationToken::new();
        polls.insert(runner, (id, ended.clone()));
        Some(PollSlot { polls: self, runner, id, ended })
    }

    /// Polls held open now.
    pub fn len(&self) -> usize {
        self.by_runner.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Drop for PollSlot<'_> {
    fn drop(&mut self) {
        let mut polls = self.polls.by_runner.lock().unwrap_or_else(PoisonError::into_inner);
        if polls.get(&self.runner).is_some_and(|(id, _)| *id == self.id) {
            polls.remove(&self.runner);
        }
    }
}

impl ActionsService {
    /// `POST poll`: a task for `runner`, or `None` once the long poll has passed, the runner
    /// polled again, or the server is shutting down. See the module documentation.
    pub async fn poll(&self, runner: &Arc<CachedRunner>, request: &PollRequest, key: Option<&str>) -> Result<Option<JobSpec>, RunnerError> {
        request.validate().map_err(|error| RunnerError::bad_request(error.message))?;
        if key.is_some_and(|key| !valid_idempotency_key(key)) {
            return Err(RunnerError::bad_request("the Idempotency-Key must be 1 to 64 letters, digits, '-' or '_'"));
        }
        if runner.disabled {
            return Err(RunnerError::forbidden("this runner is disabled and takes no new jobs; an administrator can enable it again"));
        }
        let Some(slot) = self.inner.polls.enter(runner.id, self.config().max_polls) else {
            return Err(RunnerError::unavailable("the server holds as many polls as it may; try again shortly"));
        };
        if let Some(key) = key {
            match self.resume(runner, key).await? {
                Resumed::Task(spec) => return Ok(Some(*spec)),
                // The key's task has stopped: there is nothing to resend, and the key may
                // not claim again. The runner polls anew with a new key.
                Resumed::Spent => return Ok(None),
                Resumed::Unknown => {}
            }
        }
        let deadline = tokio::time::Instant::now() + self.config().long_poll();
        loop {
            // Registered before the version is read, so a move in between still wakes it.
            let mut woken = pin!(self.inner.wake.notified());
            woken.as_mut().enable();
            let version = self.tasks_version();
            if version != runner.idle_version.load(Ordering::SeqCst) {
                match self.claim_for(runner, key).await? {
                    Claim::Task(spec) => return Ok(Some(*spec)),
                    Claim::Nothing => runner.idle_version.store(version, Ordering::SeqCst),
                    // The key went to a task that could not run: the runner polls again
                    // with a new one, and finds the rest of the queue then.
                    Claim::Spent => return Ok(None),
                }
            }
            tokio::select! {
                () = woken => {}
                () = tokio::time::sleep_until(deadline) => return Ok(None),
                () = slot.ended.cancelled() => return Ok(None),
                () = self.inner.shutdown.cancelled() => return Ok(None),
            }
        }
    }

    /// Polls held open now.
    pub fn open_polls(&self) -> usize {
        self.inner.polls.len()
    }

    /// The task a retried poll with `key` already claimed, with a fresh token, if it still
    /// runs on this runner.
    async fn resume(&self, runner: &CachedRunner, key: &str) -> Result<Resumed, RunnerError> {
        let store = self.ctx().database.actions_runs();
        let task = match runner.remembered(key) {
            Some(task) => store.task(task).await?,
            None if !runner.keys_loaded.swap(true, Ordering::SeqCst) => store.task_by_idempotency_key(runner.id, key).await?,
            None => None,
        };
        let Some(task) = task.filter(|task| task.runner_id == Some(runner.id)) else { return Ok(Resumed::Unknown) };
        if task.status != Status::Running {
            return Ok(Resumed::Spent);
        }
        let token = TaskTokenSecret::generate();
        if !store.replace_task_token(task.id, &token.digest()).await? {
            return Ok(Resumed::Spent);
        }
        let Some(job) = store.job(task.job_id).await? else { return Ok(Resumed::Spent) };
        tracing::info!(task = task.id.0, runner = runner.id.0, "poll retried: the same task with a new token");
        match self.job_spec(&job, &task, &token).await? {
            Ok(spec) => {
                self.remember_masks(&spec);
                Ok(Resumed::Task(Box::new(spec)))
            }
            Err(reason) => {
                self.fail_unrunnable(&task, &reason).await?;
                Ok(Resumed::Spent)
            }
        }
    }

    /// Runs the claim query once for `runner`; on a claim, the task's specification. A
    /// claimed job that cannot be run fails, and without an idempotency key the query runs
    /// again.
    async fn claim_for(&self, runner: &CachedRunner, key: Option<&str>) -> Result<Claim, RunnerError> {
        if runner.labels.is_empty() {
            return Ok(Claim::Nothing);
        }
        let store = self.ctx().database.actions_runs();
        loop {
            let token = TaskTokenSecret::generate();
            let request = ClaimRequest {
                runner: runner.id,
                scope: runner.scope,
                labels: &runner.labels,
                ephemeral: runner.ephemeral,
                instance_runners_for_all: self.config().instance_runners_for_all,
                token_digest: &token.digest(),
                idempotency_key: key,
                now: Timestamp::now(),
            };
            let Some(claimed) = store.claim(&request).await? else { return Ok(Claim::Nothing) };
            if let Some(key) = key {
                runner.remember(key, claimed.task.id);
            }
            tracing::info!(task = claimed.task.id.0, job = claimed.job.id.0, runner = runner.id.0, "job claimed");
            match self.job_spec(&claimed.job, &claimed.task, &token).await? {
                Ok(spec) => {
                    self.remember_masks(&spec);
                    let names: Vec<String> = spec.template.steps.iter().enumerate().map(|(index, step)| step_name(index, step)).collect();
                    store.create_steps(claimed.task.id, &names).await?;
                    self.report_statuses(claimed.job.run_id).await?;
                    return Ok(Claim::Task(Box::new(spec)));
                }
                Err(reason) => {
                    self.fail_unrunnable(&claimed.task, &reason).await?;
                    if key.is_some() {
                        return Ok(Claim::Spent);
                    }
                }
            }
        }
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
        let (secrets, vars) = if run.trusted {
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
        };
        Ok(spec.validate().map(|()| spec).map_err(|error| format!("this job cannot be handed to a runner: {error}")))
    }

    /// The `needs` context: for every needed job, its result over all its matrix instances
    /// and their outputs merged.
    pub(super) async fn needs(&self, run: &Run, template: &JobTemplate) -> Result<BTreeMap<String, Needed>, ServiceError> {
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

/// The cache scopes of a task of `run` (§12.5.6): read its own ref's scope, then the default
/// branch's; write its own — `pull/<n>` for an untrusted run, so it can poison nothing
/// another run reads first.
pub fn cache_scopes(run: &Run, default_branch: &str) -> CacheScopes {
    let write = match (run.trusted, run.pull_number) {
        (true, _) => ref_scope(&run.reference),
        (false, Some(number)) => format!("pull/{number}"),
        (false, None) => format!("untrusted/{}", run.id.0),
    };
    let default = ref_scope(&format!("refs/heads/{default_branch}"));
    let mut read = vec![write.clone()];
    if default != write {
        read.push(default);
    }
    CacheScopes { repository_id: run.repo_id.0, read, write }
}

/// A ref's cache scope: the ref without `refs/` (`heads/main`, `pull/3/head`), or a digest
/// of it when it would not make a safe path.
fn ref_scope(reference: &str) -> String {
    let scope = reference.strip_prefix("refs/").unwrap_or(reference);
    let safe = !scope.is_empty()
        && scope.len() <= 200
        && !scope.split('/').any(|part| part.is_empty() || part == "." || part == "..")
        && !scope.contains(['\\', '\0']);
    if safe {
        scope.to_owned()
    } else {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(reference.as_bytes());
        format!(
            "ref/{}",
            digest.iter().take(16).fold(String::new(), |mut hex, byte| {
                use std::fmt::Write as _;
                let _ = write!(hex, "{byte:02x}");
                hex
            })
        )
    }
}

/// The name a step is listed under: its `name`, else what it uses, else the first line of
/// its script.
fn step_name(index: usize, step: &rbit_workflow::spec::StepTemplate) -> String {
    let name = match (&step.name, &step.action) {
        (Some(name), _) if !name.trim().is_empty() => name.trim().to_owned(),
        (_, StepAction::Uses(builtin)) => format!("Run {}", builtin.uses),
        (_, StepAction::Run(script)) => script
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map_or_else(|| format!("Step {}", index + 1), |line| format!("Run {line}")),
    };
    super::runs::truncate_chars(&name, MAX_STEP_NAME_CHARS)
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
        assert!(ref_scope(&format!("refs/heads/{}", "x".repeat(300))).starts_with("ref/"));
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
