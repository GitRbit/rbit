//! Starting runs from domain events: `RepositoryPushed` starts push runs and refreshes
//! schedules, pull request events start pull request runs and cancel those of closed pull
//! requests. Evaluation (Git reads and YAML) runs on the bounded evaluation semaphore inside
//! `spawn_blocking`, never on the request path.
//!
//! * The subscriber exists only with Actions enabled (`ActionsService::spawn_background`),
//!   so a disabled instance does no work on a push at all. Enabled, a push costs the push
//!   handler nothing: the subscriber reads the event and, when Actions is on for the repository, spawns the evaluation.
//! * Only people and their deploy keys start runs ([`PushSource::starts_runs`], §11.2):
//!   mirror fetches, imports, task pushes and rbit's own ref changes never do.
//! * Discovery ([`rbit_workflow::discovery`]): at the pushed commit, the first existing of
//!   its workflow directories; at most its cap of `*.yml`/`*.yaml` files directly in it,
//!   each within `[actions] max_workflow_bytes`, read through [`GitWorkflows`]. A skip marker in the head commit message skips the
//!   push's runs. At most [`decide::MAX_REF_UPDATES`] updated refs of one push are considered, one run per workflow and ref.
//! * Evaluation holds a permit of the evaluation semaphore until its blocking work returns,
//!   and the work checks a cooperative deadline ([`EVALUATION_BUDGET`]); running out of time is a workflow error, not a stuck thread.
//! * A file that fails to parse or plan becomes a workflow-error run on the Actions page.
//! * Pull requests: `opened`, `reopened`, `synchronize`, `closed`, `edited` and
//!   `ready_for_review` are matched against `on.pull_request` (`branches` against the base
//!   branch, `paths` against the pull request's changes). Workflows come from the head
//!   commit, which `refs/pull/{n}/head` keeps in the base repository; [`super::trust`]
//!   decides what the run may touch. A `synchronize` caused by a push that may not start
//!   runs (a mirror, a task) starts none. Closing a pull request first cancels its runs that
//!   have not started (waiting, held or awaiting approval). A skip marker in the title skips its runs.
//! * A push that may start runs and moves the default branch also refreshes the
//!   repository's schedule ([`super::schedules`]), whatever its commit message says.
//! * A new repository gets its Actions settings row (`enable_for_new_repositories`) before
//!   any later event is read, so its first push already sees it.
//! * `RecvError::Lagged`: logged and counted ([`ActionsService::missed_events`]); nothing is
//!   rescanned. A push in that window can be repeated; rescanning every repository cannot.
//!
//! Which refs, events and runs count is decided by [`rbit_actions::triggers`]; this adapter
//! reads storage and Git, evaluates workflows and starts runs.
//!
//! Workstream T (triggers and runs) owns this file.

use std::{
    collections::HashMap,
    iter,
    sync::{Arc, atomic::Ordering},
};

use rbit_actions::triggers::{self as decide, EVALUATION_BUDGET, LISTED_CHANGED_PATHS, PAYLOAD_COMMITS};
use rbit_core::{
    AccountId, IssueId, IssueKind, PushSource, RepoId, Timestamp,
    actions::{Run, RunEvent},
    issue::IssueAction,
    repository::OwnedRepository,
};
use rbit_git::{CommitSummary, DiffLimits, RefUpdate, RepositoryReader};
use rbit_workflow::{
    Deadline, Event, GroupContext, LazyPaths, Limits, Loader, WorkflowError,
    discovery::{self, WorkflowFile, describe},
    model::ActivityType,
    trigger::{ChangedPaths, has_skip_marker},
};
use tokio::{
    runtime::Handle,
    sync::broadcast::{Receiver, error::RecvError},
};

use super::{
    ActionsService,
    calls::CallLoader,
    context,
    runs::{Candidate, Problem, Trigger},
    trigger_inputs::{activity_type, pull_event, pushed, run_stage},
    workflow_files::GitWorkflows,
};
use crate::{DomainEvent, ServiceError};

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
            // Actions: workflow_run and repository_dispatch.
            DomainEvent::ActionsRun { repo, run, action } => {
                let this = self.clone();
                self.inner.tasks.spawn(async move {
                    if let Err(error) = this.handle_workflow_run(repo, run, action).await {
                        tracing::warn!(%error, repo = repo.0, run = run.0, "cannot start workflow_run runs");
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

    /// Starts the runs a push asks for and returns them. Public so tests can drive it without the subscriber.
    pub async fn handle_push(&self, repo: RepoId, source: PushSource, updates: &[RefUpdate]) -> Result<Vec<Run>, ServiceError> {
        if !source.starts_runs() {
            return Ok(Vec::new());
        }
        let database = &self.ctx().database;
        if !database.actions_settings().is_enabled(repo).await? {
            return Ok(Vec::new());
        }
        let Some(owned) = database.admin().find_repository(repo).await? else { return Ok(Vec::new()) };
        let updates: Vec<RefUpdate> = decide::considered_refs(updates, pushed).into_iter().cloned().collect();
        if updates.is_empty() {
            return Ok(Vec::new());
        }
        let default_head = decide::default_branch_head(updates.iter().map(pushed), &owned.repo.default_branch).map(str::to_owned);
        if let Some(head) = default_head
            && let Err(error) = self.refresh_schedules(&owned, &head, Timestamp::now()).await
        {
            tracing::warn!(%error, repo = %owned.full_name(), "cannot refresh the repository's schedule");
        }
        let reader = self.reader(&owned);
        // A push is trusted (§11.1): its pusher could write the workflow files anyway. The
        // ref and commit are filled in per updated ref.
        let group = self.planning(&owned, RunEvent::Push, "", "", (source.pusher(), source.pusher()), true).await?;
        let (limits, default_branch) = (self.limits(), owned.repo.default_branch.clone());
        let (this, caller) = (self.clone(), owned.clone());
        let outcomes = self
            .evaluate(move |handle, deadline| {
                evaluate_push(handle, (&this, &caller), &reader, &updates, &default_branch, &group, &limits, deadline)
            })
            .await?;
        let mut started = Vec::new();
        if outcomes.is_empty() {
            return Ok(started);
        }
        let pusher = match source.pusher() {
            Some(id) => database.accounts().find_user(id).await?.map(|user| user.account),
            None => None,
        };
        let fork = crate::forks::is_fork(database, repo).await?;
        let reader = self.reader(&owned);
        let base_url = self.ctx().config.server.base_url();
        let mut allowance = self.allowance();
        for outcome in outcomes {
            let Some(after) = outcome.update.after.clone() else { continue };
            self.record_decision(repo, &after, outcome.workflows, outcome.candidates.len(), outcome.skipped).await?;
            if outcome.candidates.is_empty() {
                continue;
            }
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
                trust: rbit_actions::trust::RunTrust::Trusted,
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
        let Some((activity, pusher)) = decide::pull_trigger(pull_event(action)) else { return Ok(Vec::new()) };
        let activity = activity_type(activity);
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
        let cause = decide::pull_cause(pusher, thread.author);
        let trust = self.classify_pull_request(&owned, cause, thread.author, pull.is_cross_repository()).await?;

        let reader = self.reader(&owned);
        let reference = format!("refs/pull/{number}/head");
        let planning = self.planning(&owned, RunEvent::PullRequest, &reference, &pull.head_sha, (cause, actor), trust.is_trusted()).await?;
        let group = GroupContext {
            head_ref: pull.head_branch.clone(),
            base_ref: pull.base_branch.clone(),
            pull_request_number: u64::try_from(number).ok(),
            ..planning
        };
        let (limits, head, base_branch) = (self.limits(), pull.head_sha.clone(), pull.base_branch.clone());
        let (this, caller) = (self.clone(), owned.clone());
        let candidates = self
            .evaluate(move |handle, deadline| {
                let files = match discovery::discover(&mut GitWorkflows::new(handle, &reader, &head), &limits, deadline) {
                    Ok(files) => files,
                    Err(error) => {
                        tracing::warn!(%error, head, "cannot read the workflows of a pull request head");
                        return Vec::new();
                    }
                };
                let event = Event::PullRequest { action: &activity, base_branch: &base_branch };
                let mut changed = LazyPaths::new(|| changed_paths(handle, &reader, Some(&base_branch), &head));
                let mut loader = CallLoader::new(handle, &this, &caller, &reader, &head, &limits, deadline);
                candidates(&files, &event, &mut changed, &group, &limits, deadline, &mut loader)
            })
            .await?;
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let event = DomainEvent::Issue { repo, issue, number: thread.number, kind: IssueKind::Pull, actor, action: action.clone() };
        let base_url = self.ctx().config.server.base_url();
        let Some(payload) = crate::pulls::hooks::build(self.ctx(), &event, base_url).await? else { return Ok(Vec::new()) };
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

    /// Cancels the runs of pull request `number` that have not started: waiting, held or awaiting approval. Running ones finish.
    async fn cancel_pull_runs(&self, repo: RepoId, number: i64) -> Result<(), ServiceError> {
        let store = self.ctx().database.actions_runs();
        for run in store.unfinished_for_pull(repo, number).await? {
            if !decide::cancelled_when_pull_closes(run_stage(run.status)) {
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
    /// Agent loop: the commit's workflow files, and why none started when none did.
    workflows: usize,
    skipped: Option<&'static str>,
}

/// Blocking: the candidates of every updated ref of a push.
#[expect(clippy::too_many_arguments, reason = "the evaluation's parts, all borrowed from its closure")]
fn evaluate_push(
    handle: &Handle,
    (service, caller): (&ActionsService, &OwnedRepository),
    reader: &RepositoryReader,
    updates: &[RefUpdate],
    default_branch: &str,
    planning: &GroupContext,
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
        if has_skip_marker(&decide::head_message(&head.summary, head.body.as_deref())) {
            tracing::debug!(r#ref = %update.name, "skip marker in the head commit; no runs");
            outcomes.push(PushOutcome { update: update.clone(), head, candidates: Vec::new(), workflows: 0, skipped: Some("skip_marker") });
            continue;
        }
        if !by_commit.contains_key(after) {
            match discovery::discover(&mut GitWorkflows::new(handle, reader, after), limits, deadline) {
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
            outcomes.push(PushOutcome { update: update.clone(), head, candidates: Vec::new(), workflows: 0, skipped: Some("no_workflows") });
            continue;
        }
        let base = decide::comparison_base(pushed(update), default_branch);
        let mut changed = LazyPaths::new(|| changed_paths(handle, reader, base, after));
        let event = Event::Push { reference: &update.name, deleted: false };
        let group = GroupContext { reference: update.name.clone(), sha: after.clone(), ..planning.clone() };
        let mut loader = CallLoader::new(handle, service, caller, reader, after, limits, deadline);
        let candidates = candidates(files, &event, &mut changed, &group, limits, deadline, &mut loader);
        let skipped = candidates.is_empty().then_some("filtered");
        outcomes.push(PushOutcome { update: update.clone(), head, candidates, workflows: files.len(), skipped });
    }
    outcomes
}

/// Blocking: the paths changed from `base` to `head`, with both sides of renames; `None`
/// without a base, when the comparison fails, or when [`decide::changed_paths_known`] refuses the list — then `paths` filters pass.
pub(super) fn changed_paths(handle: &Handle, reader: &RepositoryReader, base: Option<&str>, head: &str) -> Option<Vec<String>> {
    let base = base?;
    let limits = DiffLimits { max_files: LISTED_CHANGED_PATHS, max_file_bytes: 0, max_total_lines: 0, max_total_bytes: 0 };
    let comparison = match handle.block_on(reader.compare(base.to_owned(), head.to_owned(), limits, 0)) {
        Ok(comparison) => comparison,
        Err(error) => {
            tracing::debug!(%error, base, head, "cannot list changed paths; paths filters pass");
            return None;
        }
    };
    if !decide::changed_paths_known(comparison.files.len(), comparison.merge_base.is_some()) {
        return None;
    }
    Some(comparison.files.into_iter().flat_map(|file| iter::once(file.path).chain(file.old_path)).collect())
}

/// What each workflow of `files` makes of `event` ([`discovery::evaluate`]), with problems as workflow-error runs record them.
pub(super) fn candidates(
    files: &[WorkflowFile],
    event: &Event<'_>,
    changed: &mut dyn ChangedPaths,
    group: &GroupContext,
    limits: &Limits,
    deadline: &Deadline,
    loader: &mut dyn Loader,
) -> Vec<Candidate> {
    discovery::evaluate(files, event, changed, group, limits, deadline, loader)
        .into_iter()
        .map(|evaluated| Candidate {
            outcome: evaluated.outcome.map_err(|error| problem(&evaluated.path, &error)),
            path: evaluated.path,
            name: evaluated.name,
        })
        .collect()
}

// Actions: metrics.
/// `error` of the workflow at `path` as a workflow-error run records it.
pub(super) fn problem(path: &str, error: &WorkflowError) -> Problem {
    Problem { reason: error.kind, message: describe(path, error) }
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
