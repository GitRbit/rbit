//! Starting runs from domain events: `RepositoryPushed` starts push runs and refreshes
//! schedules, pull request events start pull request runs and cancel those of closed pull
//! requests. Evaluation (Git reads and YAML) runs on the bounded evaluation semaphore inside
//! `spawn_blocking`, never on the request path.
//!
//! * The subscriber exists only with Actions enabled (`ActionsService::spawn_background`),
//!   so a disabled instance does no work on a push at all. Enabled, a push costs the push
//!   handler nothing: the subscriber reads the event and, when Actions is on for the
//!   repository, spawns the evaluation.
//! * Only people and their deploy keys start runs ([`PushSource::starts_runs`], §11.2):
//!   mirror fetches, imports, task pushes and rbit's own ref changes never do.
//! * Discovery: at the pushed commit, the first existing of [`WORKFLOW_DIRS`]; at most
//!   [`MAX_WORKFLOW_FILES`] `*.yml`/`*.yaml` files directly in it, each within
//!   `[actions] max_workflow_bytes`. A skip marker in the head commit message skips the
//!   push's runs. At most [`MAX_REF_UPDATES`] updated refs of one push are considered, one
//!   run per workflow and ref.
//! * Evaluation holds a permit of the evaluation semaphore until its blocking work returns,
//!   and the work checks a cooperative deadline ([`EVALUATION_BUDGET`]); running out of
//!   time is a workflow error, not a stuck thread.
//! * A file that fails to parse or plan becomes a workflow-error run on the Actions page.
//! * Pull requests: `opened`, `reopened`, `synchronize`, `closed`, `edited` and
//!   `ready_for_review` are matched against `on.pull_request` (`branches` against the base
//!   branch, `paths` against the pull request's changes). Workflows come from the head
//!   commit, which `refs/pull/{n}/head` keeps in the base repository; [`super::trust`]
//!   decides what the run may touch. A `synchronize` caused by a push that may not start
//!   runs (a mirror, a task) starts none. Closing a pull request first cancels its runs that
//!   have not started (waiting, held or awaiting approval). A skip marker in the title skips
//!   its runs.
//! * A new repository gets its Actions settings row (`enable_for_new_repositories`) before
//!   any later event is read, so its first push already sees it.
//! * `RecvError::Lagged`: logged and counted ([`ActionsService::missed_events`]); nothing is
//!   rescanned. A push in that window can be repeated; rescanning every repository cannot.
//!
//! Workstream T (triggers and runs) owns this file.

use std::{
    collections::HashMap,
    iter,
    path::Path,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use rbit_core::{
    AccountId, IssueId, IssueKind, PushSource, RepoId, Timestamp,
    actions::{Run, RunEvent, RunTrust, Status},
    issue::IssueAction,
    repository::OwnedRepository,
};
use rbit_git::{CommitSummary, DiffLimits, EntryKind, GitError, RefUpdate, RepositoryReader};
use rbit_workflow::{
    Deadline, ErrorKind, Event, GroupContext, LazyPaths, Limits, Workflow, WorkflowError,
    model::ActivityType,
    trigger::{ChangedPaths, has_skip_marker},
};
use tokio::{
    runtime::Handle,
    sync::broadcast::{Receiver, error::RecvError},
};

use super::{
    ActionsService, context,
    runs::{Candidate, Plan, Trigger},
};
use crate::{DomainEvent, ServiceError};

/// Where workflows live, in precedence order: a repository with both uses only the first.
pub const WORKFLOW_DIRS: [&str; 2] = [".forgejo/workflows", ".github/workflows"];
/// Most workflow files read from the directory.
pub const MAX_WORKFLOW_FILES: usize = 64;
/// Updated refs of one push that are considered.
pub const MAX_REF_UPDATES: usize = 16;
/// Changed paths listed for `paths` filters; beyond it, `paths` filters pass (as on GitHub).
pub const MAX_CHANGED_PATHS: usize = 3_000;
/// How long one evaluation may take before it gives up.
pub const EVALUATION_BUDGET: Duration = Duration::from_secs(10);
/// Commits listed in a push run's event payload, as in the `push` webhook.
const PAYLOAD_COMMITS: usize = 20;

/// A workflow file read at a commit.
#[derive(Debug)]
pub struct WorkflowFile {
    /// `.forgejo/workflows/ci.yml`.
    pub path: String,
    pub parsed: Result<Workflow, WorkflowError>,
}

impl WorkflowFile {
    /// The workflow's `name`, or its path when it has none (or does not parse).
    pub fn name(&self) -> String {
        match &self.parsed {
            Ok(workflow) => workflow.name.clone().filter(|name| !name.trim().is_empty()).unwrap_or_else(|| self.path.clone()),
            Err(_) => self.path.clone(),
        }
    }
}

impl ActionsService {
    /// Reads domain events until shutdown (see the module documentation).
    pub(super) async fn follow_events(self, mut events: Receiver<DomainEvent>) {
        loop {
            let event = tokio::select! {
                () = self.inner.shutdown.cancelled() => return,
                event = events.recv() => event,
            };
            match event {
                Ok(event) => self.on_event(event).await,
                Err(RecvError::Lagged(skipped)) => {
                    self.inner.missed_events.fetch_add(skipped, Ordering::Relaxed);
                    tracing::warn!(skipped, "Actions missed domain events; pushes in that window started no runs (push again to run them)");
                }
                Err(RecvError::Closed) => return,
            }
        }
    }

    /// Domain events Actions missed because it fell behind, since start-up.
    pub fn missed_events(&self) -> u64 {
        self.inner.missed_events.load(Ordering::Relaxed)
    }

    async fn on_event(&self, event: DomainEvent) {
        match event {
            DomainEvent::RepositoryCreated { repo, .. } | DomainEvent::RepositoryForked { repo, .. } => {
                if let Err(error) = self.record_new_repository(repo).await {
                    tracing::warn!(%error, repo = repo.0, "cannot store the Actions settings of a new repository");
                }
            }
            DomainEvent::RepositoryPushed { repo, source, updates } => {
                let this = self.clone();
                self.inner.tasks.spawn(async move {
                    if let Err(error) = this.handle_push(repo, source, &updates).await {
                        tracing::warn!(%error, repo = repo.0, "cannot start workflow runs for a push");
                    }
                });
            }
            DomainEvent::Issue { repo, issue, kind: IssueKind::Pull, actor, action, .. } => {
                let this = self.clone();
                self.inner.tasks.spawn(async move {
                    if let Err(error) = this.handle_pull_request(repo, issue, actor, &action).await {
                        tracing::warn!(%error, repo = repo.0, "cannot start workflow runs for a pull request");
                    }
                });
            }
            _ => {}
        }
    }

    /// Stores a new repository's Actions switch: `[actions] enable_for_new_repositories`.
    pub async fn record_new_repository(&self, repo: RepoId) -> Result<(), ServiceError> {
        let enabled = self.config().enable_for_new_repositories;
        self.ctx().database.actions_settings().set_enabled(repo, enabled, Timestamp::now()).await?;
        Ok(())
    }

    /// Starts the runs a push asks for and returns them. Public so tests can drive it
    /// without the subscriber.
    pub async fn handle_push(&self, repo: RepoId, source: PushSource, updates: &[RefUpdate]) -> Result<Vec<Run>, ServiceError> {
        if !source.starts_runs() {
            return Ok(Vec::new());
        }
        let database = &self.ctx().database;
        if !database.actions_settings().is_enabled(repo).await? {
            return Ok(Vec::new());
        }
        let Some(owned) = database.admin().find_repository(repo).await? else { return Ok(Vec::new()) };
        let updates: Vec<RefUpdate> = updates
            .iter()
            .filter(|update| !update.is_deleted() && (update.branch().is_some() || update.tag().is_some()))
            .take(MAX_REF_UPDATES)
            .cloned()
            .collect();
        if updates.is_empty() {
            return Ok(Vec::new());
        }
        let reader = self.reader(&owned);
        let (limits, default_branch, full_name) = (self.limits(), owned.repo.default_branch.clone(), owned.full_name());
        let outcomes = self
            .evaluate(move |handle, deadline| evaluate_push(handle, &reader, &updates, &default_branch, &full_name, &limits, deadline))
            .await?;
        let mut started = Vec::new();
        if outcomes.is_empty() {
            return Ok(started);
        }
        let pusher = match source.pusher() {
            Some(id) => database.accounts().find_user(id).await?.map(|user| user.account),
            None => None,
        };
        let fork = database.forks().forks_among(&[repo]).await?.contains(&repo);
        let reader = self.reader(&owned);
        let base_url = self.ctx().config.server.base_url();
        let mut allowance = self.allowance();
        for outcome in outcomes {
            let Some(after) = outcome.update.after.clone() else { continue };
            let commits = pushed_commits(&reader, &outcome.update, &after).await;
            let payload = context::push_event(&owned, &outcome.update, &commits, pusher.as_ref(), fork, base_url);
            let payload = context::fit_payload(payload);
            let trigger = Trigger {
                repo: &owned,
                event: RunEvent::Push,
                reference: &outcome.update.name,
                sha: &after,
                title: &outcome.head.summary,
                actor: source.pusher(),
                trigger_actor: source.pusher(),
                pull_number: None,
                is_fork: false,
                trust: RunTrust::Trusted,
                payload: &payload,
                inputs: None,
            };
            for candidate in outcome.candidates {
                started.extend(self.start_run(&trigger, candidate, &mut allowance).await?);
            }
        }
        Ok(started)
    }

    /// Starts the runs a pull request event asks for (`actor` is the event's actor) and
    /// returns them; closing also cancels the pull request's runs that have not started.
    /// Public so tests can drive it without the subscriber.
    pub async fn handle_pull_request(
        &self,
        repo: RepoId,
        issue: IssueId,
        actor: Option<AccountId>,
        action: &IssueAction,
    ) -> Result<Vec<Run>, ServiceError> {
        let (activity, pusher) = match action {
            IssueAction::Opened => (ActivityType::Opened, None),
            IssueAction::Reopened => (ActivityType::Reopened, None),
            IssueAction::Edited { .. } => (ActivityType::Edited, None),
            IssueAction::ReadyForReview => (ActivityType::ReadyForReview, None),
            IssueAction::Closed { .. } => (ActivityType::Closed, None),
            IssueAction::Synchronized { source, .. } if source.starts_runs() => (ActivityType::Synchronize, Some(source.pusher())),
            _ => return Ok(Vec::new()),
        };
        let database = &self.ctx().database;
        if !database.actions_settings().is_enabled(repo).await? {
            return Ok(Vec::new());
        }
        let Some(owned) = database.admin().find_repository(repo).await? else { return Ok(Vec::new()) };
        let (Some(thread), Some(pull)) = (database.issues().find(issue).await?, database.pulls().find(issue).await?) else {
            return Ok(Vec::new());
        };
        let number = i64::from(thread.number.get());
        if activity == ActivityType::Closed {
            self.cancel_pull_runs(repo, number).await?;
        }
        if has_skip_marker(thread.title.as_str()) {
            return Ok(Vec::new());
        }
        // The author for everything but `synchronize`, whose actor pushed the new head.
        let cause = pusher.unwrap_or(thread.author);
        let trust = self.classify_pull_request(&owned, cause, thread.author).await?;

        let reader = self.reader(&owned);
        let reference = format!("refs/pull/{number}/head");
        let group = GroupContext {
            reference: reference.clone(),
            head_ref: pull.head_branch.clone(),
            base_ref: pull.base_branch.clone(),
            event_name: RunEvent::PullRequest.as_str().to_owned(),
            repository: owned.full_name(),
            pull_request_number: u64::try_from(number).ok(),
            ..GroupContext::default()
        };
        let (limits, head, base_branch) = (self.limits(), pull.head_sha.clone(), pull.base_branch.clone());
        let candidates = self
            .evaluate(move |handle, deadline| {
                let files = match discover(handle, &reader, &head, &limits, deadline) {
                    Ok(files) => files,
                    Err(error) => {
                        tracing::warn!(%error, head, "cannot read the workflows of a pull request head");
                        return Vec::new();
                    }
                };
                let event = Event::PullRequest { action: &activity, base_branch: &base_branch };
                let mut changed = LazyPaths::new(|| changed_paths(handle, &reader, Some(&base_branch), &head));
                candidates(&files, &event, &mut changed, &group, &limits, deadline)
            })
            .await?;
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let event = DomainEvent::Issue { repo, issue, number: thread.number, kind: IssueKind::Pull, actor, action: action.clone() };
        let base_url = self.ctx().config.server.base_url();
        let Some(payload) = crate::pulls::hooks::payload(self.ctx(), &event, base_url).await? else { return Ok(Vec::new()) };
        let payload = context::fit_payload(payload.body);
        let trigger = Trigger {
            repo: &owned,
            event: RunEvent::PullRequest,
            reference: &reference,
            sha: &pull.head_sha,
            title: thread.title.as_str(),
            actor: cause,
            trigger_actor: actor,
            pull_number: Some(number),
            is_fork: pull.is_cross_repository(),
            trust,
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

    /// Cancels the runs of pull request `number` that have not started: waiting, held or
    /// awaiting approval. Running ones finish.
    async fn cancel_pull_runs(&self, repo: RepoId, number: i64) -> Result<(), ServiceError> {
        let store = self.ctx().database.actions_runs();
        for run in store.unfinished_for_pull(repo, number).await? {
            if !matches!(run.status, Status::Waiting | Status::Blocked | Status::AwaitingApproval) {
                continue;
            }
            if let Some(settled) = store.cancel_run(repo, run.id, Timestamp::now()).await? {
                tracing::info!(repo = repo.0, run = run.id.0, pull = number, "workflow run of a closed pull request cancelled");
                self.settled(&settled).await?;
            }
        }
        Ok(())
    }

    /// A reader of `repo`'s Git data.
    pub(super) fn reader(&self, repo: &OwnedRepository) -> RepositoryReader {
        self.ctx().repositories.reader(repo.repo.owner_id, repo.repo.id)
    }

    /// Runs `work` on the blocking pool with an evaluation permit and a fresh
    /// [`EVALUATION_BUDGET`]. The permit moves into the blocking task, so it is held until
    /// `work` returns even if nobody waits for it any more.
    pub(super) async fn evaluate<T, F>(&self, work: F) -> Result<T, ServiceError>
    where
        T: Send + 'static,
        F: FnOnce(&Handle, &Deadline) -> T + Send + 'static,
    {
        let permit = tokio::select! {
            permit = Arc::clone(self.evaluations()).acquire_owned() => permit.map_err(ServiceError::internal)?,
            () = self.inner.shutdown.cancelled() => return Err(ServiceError::internal(std::io::Error::other("Actions is shutting down"))),
        };
        let handle = Handle::current();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            work(&handle, &Deadline::after(EVALUATION_BUDGET))
        })
        .await
        .map_err(ServiceError::internal)
    }
}

/// What one updated ref of a push asks for.
#[derive(Debug)]
struct PushOutcome {
    update: RefUpdate,
    head: CommitSummary,
    candidates: Vec<Candidate>,
}

/// Blocking: the candidates of every updated ref of a push.
fn evaluate_push(
    handle: &Handle,
    reader: &RepositoryReader,
    updates: &[RefUpdate],
    default_branch: &str,
    repository: &str,
    limits: &Limits,
    deadline: &Deadline,
) -> Vec<PushOutcome> {
    let mut by_commit: HashMap<String, Vec<WorkflowFile>> = HashMap::new();
    let mut outcomes = Vec::new();
    for update in updates {
        let Some(after) = &update.after else { continue };
        let head = match handle.block_on(reader.history(after.clone(), 0, 1)) {
            Ok(mut commits) => match commits.pop() {
                Some(head) => head,
                None => continue,
            },
            Err(error) => {
                tracing::warn!(%error, r#ref = %update.name, "cannot read a pushed commit");
                continue;
            }
        };
        if has_skip_marker(&message(&head)) {
            tracing::debug!(r#ref = %update.name, "skip marker in the head commit; no runs");
            continue;
        }
        if !by_commit.contains_key(after) {
            match discover(handle, reader, after, limits, deadline) {
                Ok(files) => {
                    by_commit.insert(after.clone(), files);
                }
                Err(error) => {
                    tracing::warn!(%error, r#ref = %update.name, "cannot read the workflows of a pushed commit");
                    continue;
                }
            }
        }
        let Some(files) = by_commit.get(after) else { continue };
        if files.is_empty() {
            continue;
        }
        // A new branch is compared with the default branch, which it most likely left.
        let base = match (&update.before, update.branch()) {
            (Some(before), _) => Some(before.clone()),
            (None, Some(branch)) if branch != default_branch => Some(default_branch.to_owned()),
            (None, _) => None,
        };
        let mut changed = LazyPaths::new(|| changed_paths(handle, reader, base.as_deref(), after));
        let event = Event::Push { reference: &update.name, deleted: false };
        let group = GroupContext {
            reference: update.name.clone(),
            event_name: RunEvent::Push.as_str().to_owned(),
            repository: repository.to_owned(),
            ..GroupContext::default()
        };
        let candidates = candidates(files, &event, &mut changed, &group, limits, deadline);
        if !candidates.is_empty() {
            outcomes.push(PushOutcome { update: update.clone(), head, candidates });
        }
    }
    outcomes
}

/// The full message of `commit`, where skip markers are looked for.
fn message(commit: &CommitSummary) -> String {
    match &commit.body {
        Some(body) => format!("{}\n\n{body}", commit.summary),
        None => commit.summary.clone(),
    }
}

/// Blocking: the workflow files of commit `sha`, parsed (see the module documentation).
pub(super) fn discover(
    handle: &Handle,
    reader: &RepositoryReader,
    sha: &str,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<Vec<WorkflowFile>, GitError> {
    for dir in WORKFLOW_DIRS {
        let tree = match handle.block_on(reader.tree(sha.to_owned(), dir.to_owned())) {
            Ok(tree) => tree,
            Err(GitError::NotFound(_)) => continue,
            Err(error) => return Err(error),
        };
        let mut names: Vec<String> = tree
            .entries
            .into_iter()
            .filter(|entry| matches!(entry.kind, EntryKind::File | EntryKind::Executable))
            .map(|entry| entry.name)
            .filter(|name| Path::new(name).extension().is_some_and(|extension| extension == "yml" || extension == "yaml"))
            .collect();
        names.sort();
        if names.len() > MAX_WORKFLOW_FILES {
            tracing::warn!(dir, files = names.len(), "only the first {MAX_WORKFLOW_FILES} workflow files are read");
            names.truncate(MAX_WORKFLOW_FILES);
        }
        return Ok(names
            .into_iter()
            .map(|name| {
                let path = format!("{dir}/{name}");
                let parsed = read_workflow(handle, reader, sha, &path, limits, deadline);
                WorkflowFile { path, parsed }
            })
            .collect());
    }
    Ok(Vec::new())
}

fn read_workflow(
    handle: &Handle,
    reader: &RepositoryReader,
    sha: &str,
    path: &str,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<Workflow, WorkflowError> {
    deadline.check()?;
    let blob = handle.block_on(reader.blob(sha.to_owned(), path.to_owned(), limits.max_bytes)).map_err(|error| {
        tracing::warn!(%error, path, "cannot read a workflow file");
        WorkflowError::new(ErrorKind::Parse, "the file could not be read")
    })?;
    let Some(data) = blob.data else {
        return Err(WorkflowError::limit(format!("the file is larger than {} bytes", limits.max_bytes), None));
    };
    rbit_workflow::parse(&data, limits, deadline)
}

/// Blocking: the paths changed from `base` to `head`, with both sides of renames; `None`
/// without a base, when the comparison fails, or beyond [`MAX_CHANGED_PATHS`] — then
/// `paths` filters pass.
pub(super) fn changed_paths(handle: &Handle, reader: &RepositoryReader, base: Option<&str>, head: &str) -> Option<Vec<String>> {
    let base = base?;
    let limits = DiffLimits { max_files: MAX_CHANGED_PATHS + 1, max_file_bytes: 0, max_total_lines: 0 };
    let comparison = match handle.block_on(reader.compare(base.to_owned(), head.to_owned(), limits, 0)) {
        Ok(comparison) => comparison,
        Err(error) => {
            tracing::debug!(%error, base, head, "cannot list changed paths; paths filters pass");
            return None;
        }
    };
    if comparison.files.len() > MAX_CHANGED_PATHS {
        return None;
    }
    Some(comparison.files.into_iter().flat_map(|file| iter::once(file.path).chain(file.old_path)).collect())
}

/// What each workflow of `files` makes of `event`: workflows it starts are planned, files
/// that do not parse are reported.
pub(super) fn candidates(
    files: &[WorkflowFile],
    event: &Event<'_>,
    changed: &mut dyn ChangedPaths,
    group: &GroupContext,
    limits: &Limits,
    deadline: &Deadline,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for file in files {
        let outcome = match &file.parsed {
            Err(error) => Err(describe(&file.path, error)),
            Ok(workflow) if rbit_workflow::matches(workflow, event, changed) => {
                plan_workflow(workflow, &file.name(), group, limits, deadline).map_err(|error| describe(&file.path, &error))
            }
            Ok(_) => continue,
        };
        candidates.push(Candidate { path: file.path.clone(), name: file.name(), outcome });
    }
    candidates
}

/// A workflow error as the run shows it: the file, then the problem and where it is.
pub(super) fn describe(path: &str, error: &WorkflowError) -> String {
    format!("{path}: {error}")
}

/// Plans `workflow` (named `name`) for an event described by `group`.
pub(super) fn plan_workflow(
    workflow: &Workflow,
    name: &str,
    group: &GroupContext,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<Plan, WorkflowError> {
    let jobs = rbit_workflow::plan(workflow, limits, deadline)?;
    let context = GroupContext { workflow: name.to_owned(), run_id: 0, ..group.clone() };
    let concurrency = match rbit_workflow::concurrency_group(workflow, &context)? {
        // A group naming `github.run_id` is unique to its run: it never waits for or
        // cancels another, so it is no group at all. The id is not known before the run
        // exists, so evaluating twice tells.
        Some((group, cancel)) => match rbit_workflow::concurrency_group(workflow, &GroupContext { run_id: 1, ..context })? {
            Some((other, _)) if other == group => Some((group, cancel)),
            _ => None,
        },
        None => None,
    };
    Ok(Plan { jobs, concurrency })
}

/// The commits a ref update brought, newest first, for the push payload.
async fn pushed_commits(reader: &RepositoryReader, update: &RefUpdate, after: &str) -> Vec<CommitSummary> {
    let excluding = match &update.before {
        Some(before) => vec![before.clone()],
        None => match reader.ref_snapshot().await {
            Ok(snapshot) => {
                let mut tips: Vec<String> = snapshot.ids().map(str::to_owned).collect();
                if let Some(own) = tips.iter().position(|tip| tip == after) {
                    tips.remove(own);
                }
                tips
            }
            Err(_) => Vec::new(),
        },
    };
    reader.new_commits(after.to_owned(), excluding, PAYLOAD_COMMITS).await.unwrap_or_default()
}
