//! The merge queue (agent plan step 4.1): nothing lands untested.
//!
//! * `enqueue` puts a pull request in line for its base branch when everything but its
//!   checks would let it merge now (open, not a draft, no conflicts, approvals, …), for the
//!   head the caller saw. Its place in line is reported.
//! * One entry of a line is tested at a time. The server builds its merge (by the chosen
//!   method) on the current base tip into `refs/merge-queue/<base>/pr-<n>`; workflows see a
//!   push to the branch `merge-queue/<base>/pr-<n>`, so a workflow without a branch filter
//!   tests it and one limited to `main` (a deploy) does not run.
//! * When every context the base branch's protection requires has passed on that exact
//!   commit, it lands by compare-and-swap of the base branch: the tested commit is what the
//!   base points at afterwards, or nothing changes. A base that moved meanwhile means a new
//!   candidate and a new test. A failed required check, a conflict or a moved head takes the
//!   entry out of line with the reason; the next one starts.
//! * With no required context, the candidate lands as soon as it is built (the plain merge
//!   would have, too).
//!
//! The queue advances on commit statuses and pushes of its repository, and on a sweep every
//! [`QUEUE_SWEEP`] in case an event was missed.

use std::time::Duration;

use rbit_core::{
    IssueNumber, PushSource, RepoId, StateReason, TimelineEvent, Timestamp, User,
    issue::MergeStrategy,
    pull::{CommitIdentity, Requirement, noreply_domain},
    status::StatusState,
};
use rbit_git::merge::{
    MergeError, MergeSpec,
    candidate::{candidate, candidate_ref, drop_candidate, land},
};
use rbit_storage::sqlite::{
    merge_queue::{NewEntry, QueueEntry},
    pulls::MergeRecord,
};

use super::{MergeFailure, PullContext, PullService, merge_lock};
use crate::{RepoContext, ServiceError};

/// How often every line with entries is looked at again.
pub const QUEUE_SWEEP: Duration = Duration::from_secs(30);

/// A pull request's place in its line.
#[derive(Debug, Clone)]
pub struct QueuePlace {
    pub entry: QueueEntry,
    /// 1 for the entry being tested (or next); `None` once it left the line.
    pub position: Option<usize>,
}

fn strategy_name(strategy: MergeStrategy) -> &'static str {
    match strategy {
        MergeStrategy::Merge => "merge",
        MergeStrategy::Squash => "squash",
        MergeStrategy::Rebase => "rebase",
    }
}

fn strategy_of(name: &str) -> MergeStrategy {
    match name {
        "squash" => MergeStrategy::Squash,
        "rebase" => MergeStrategy::Rebase,
        _ => MergeStrategy::Merge,
    }
}

impl PullService {
    /// Queues `ctx` to land with `strategy` for head `expected_head` (the current one when
    /// `None`). Every requirement but its checks must be met. Queuing a pull request that is
    /// in line already reports its place.
    pub async fn enqueue(
        &self,
        actor: &User,
        ctx: &PullContext,
        strategy: MergeStrategy,
        expected_head: Option<&str>,
    ) -> Result<QueuePlace, MergeFailure> {
        if !ctx.repo().can_write_content() {
            return Err(ServiceError::Forbidden.into());
        }
        if expected_head.is_some_and(|expected| !ctx.pull.head_sha.starts_with(expected)) {
            return Err(MergeFailure::HeadChanged);
        }
        let merge_box = self.merge_box(Some(actor), ctx).await?;
        let unmet: Vec<String> = merge_box
            .evaluation
            .checks
            .iter()
            .filter(|check| !check.met && !matches!(check.requirement, Requirement::RequiredChecks(_) | Requirement::Checks(_)))
            .map(|check| check.requirement.describe(false))
            .collect();
        if !unmet.is_empty() {
            return Err(MergeFailure::Blocked(unmet.join("; ")));
        }
        let store = self.ctx().database.merge_queue();
        let new = NewEntry {
            repo: ctx.repo().repo.repo.id,
            issue: ctx.issue().id,
            number: ctx.issue().number.get(),
            base_branch: &ctx.pull.base_branch,
            head_sha: &ctx.pull.head_sha,
            strategy: strategy_name(strategy),
            queued_by: actor.id(),
        };
        if store.enqueue(&new, Timestamp::now()).await?.is_some() {
            tracing::info!(repo = %ctx.repo().repo.full_name(), number = ctx.issue().number.get(), "pull request queued to land");
            self.nudge(ctx.repo().repo.repo.id, ctx.pull.base_branch.clone());
        }
        Ok(self.queue_place(ctx).await?.ok_or(ServiceError::NotFound)?)
    }

    /// Where `ctx` stands in its line: its newest entry, live or not.
    pub async fn queue_place(&self, ctx: &PullContext) -> Result<Option<QueuePlace>, ServiceError> {
        let store = self.ctx().database.merge_queue();
        let Some(entry) = store.of_pull(ctx.issue().id).await? else { return Ok(None) };
        let line = store.line(entry.repo, &entry.base_branch).await?;
        let position = line.iter().position(|live| live.id == entry.id).map(|index| index + 1);
        Ok(Some(QueuePlace { entry, position }))
    }

    /// Takes `ctx` out of line; `false` when it was not in line.
    pub async fn dequeue(&self, actor: &User, ctx: &PullContext) -> Result<bool, ServiceError> {
        if !ctx.repo().can_write_content() {
            return Err(ServiceError::Forbidden);
        }
        let store = self.ctx().database.merge_queue();
        let Some(entry) = store.of_pull(ctx.issue().id).await? else { return Ok(false) };
        let removed = store.finish(entry.id, "removed", Some(&format!("taken out of line by {}", actor.name())), Timestamp::now()).await?;
        if removed {
            self.forget_candidate(ctx.repo(), &entry).await;
            self.nudge(entry.repo, entry.base_branch);
        }
        Ok(removed)
    }

    /// Advances `repo`'s line into `base` in the background.
    pub(super) fn nudge(&self, repo: RepoId, base: String) {
        let this = self.clone();
        self.inner.tasks.spawn(async move {
            if let Err(error) = this.advance(repo, &base).await {
                tracing::warn!(%error, repo = repo.0, base, "cannot advance the merge queue");
            }
        });
    }

    /// Advances every line of `repo`.
    pub(super) async fn advance_repo(&self, repo: RepoId) {
        let lines = self.ctx().database.merge_queue().active_lines().await;
        for (line_repo, base) in lines.unwrap_or_default().into_iter().filter(|(line_repo, _)| *line_repo == repo) {
            if let Err(error) = self.advance(line_repo, &base).await {
                tracing::warn!(%error, repo = repo.0, base, "cannot advance the merge queue");
            }
        }
    }

    /// The sweep: every line with entries, every [`QUEUE_SWEEP`], until shutdown.
    pub(super) async fn queue_sweeper(self) {
        loop {
            tokio::select! {
                () = self.inner.shutdown.cancelled() => return,
                () = tokio::time::sleep(QUEUE_SWEEP) => {}
            }
            for (repo, base) in self.ctx().database.merge_queue().active_lines().await.unwrap_or_default() {
                if let Err(error) = self.advance(repo, &base).await {
                    tracing::warn!(%error, repo = repo.0, base, "cannot advance the merge queue");
                }
            }
        }
    }

    /// One step of `repo`'s line into `base`: start, judge or land its first entry.
    async fn advance(&self, repo: RepoId, base: &str) -> Result<(), ServiceError> {
        let lock = merge_lock(&self.inner.merge_locks, repo);
        let _guard = lock.lock().await;
        let store = self.ctx().database.merge_queue();
        let Some(entry) = store.line(repo, base).await?.into_iter().next() else { return Ok(()) };
        let database = &self.ctx().database;
        let Some(user) = database.accounts().find_user(entry.queued_by).await? else {
            return self.leave(&entry, "failed", "whoever queued it is gone").await;
        };
        let Some(owned) = database.admin().find_repository(repo).await? else { return Ok(()) };
        let repo_ctx = self.inner.repositories.open(Some(&user), owned.owner.name.as_str(), owned.repo.name.as_str()).await?;
        let number = IssueNumber::new(entry.number).ok_or(ServiceError::NotFound)?;
        let ctx = self.open(Some(&user), repo_ctx, number).await?;
        if !ctx.issue().is_open() {
            return self.leave(&entry, "removed", "the pull request is no longer open").await;
        }
        if ctx.pull.head_sha != entry.head_sha {
            return self.leave(&entry, "failed", "the head moved after it was queued; queue it again").await;
        }
        let Some(base_tip) = self.base_tip(&ctx).await? else {
            return self.leave(&entry, "failed", "the base branch does not exist").await;
        };
        match (entry.state.as_str(), entry.candidate_sha.as_deref(), entry.base_sha.as_deref()) {
            ("testing", Some(candidate_sha), Some(built_on)) if built_on == base_tip => {
                let rules = self.merge_box(Some(&user), &ctx).await?.rules;
                let statuses = database.commit_statuses().latest(repo, candidate_sha).await?;
                let mut pending = false;
                for context in &rules.required_contexts {
                    match statuses.iter().find(|status| status.context.as_str() == context.as_str()).map(|status| status.state) {
                        Some(StatusState::Success) => {}
                        Some(StatusState::Failure | StatusState::Error) => {
                            let reason = format!("`{context}` failed on the candidate {}", &candidate_sha[..candidate_sha.len().min(12)]);
                            return self.leave(&entry, "failed", &reason).await;
                        }
                        Some(StatusState::Pending) | None => pending = true,
                    }
                }
                if pending { Ok(()) } else { self.land_entry(&user, &ctx, &entry, candidate_sha, &base_tip).await }
            }
            _ => self.build_entry(&user, &ctx, &entry, &base_tip).await,
        }
    }

    /// Builds `entry`'s candidate on `base_tip` and reports it as a push, which starts CI.
    async fn build_entry(&self, user: &User, ctx: &PullContext, entry: &QueueEntry, base_tip: &str) -> Result<(), ServiceError> {
        let strategy = strategy_of(&entry.strategy);
        let merge_box = self.merge_box(Some(user), ctx).await?;
        let message = match strategy {
            MergeStrategy::Squash => format!("{}\n\n{}", merge_box.default_squash_title, merge_box.default_squash_message),
            _ => format!("{}\n\n{}", merge_box.default_title, merge_box.default_message),
        };
        let config = &self.ctx().config;
        let domain = noreply_domain(&config.pulls.noreply_domain, &config.server.public_url);
        let merger = CommitIdentity::noreply(user.name().as_str(), user.account.display_name.as_deref(), &domain);
        let author = match ctx.thread.author.as_ref() {
            Some(author) => CommitIdentity::noreply(author.name.as_str(), author.display_name.as_deref(), &domain),
            None => CommitIdentity::noreply("ghost", None, &domain),
        };
        let spec = MergeSpec {
            repo: &ctx.repo().disk_path,
            base_branch: &ctx.pull.base_branch,
            base_tip,
            head: &ctx.pull.head_sha,
            strategy,
            message: message.trim_end(),
            author: &author,
            merger: &merger,
            at: Timestamp::now(),
        };
        let target = candidate_ref(&ctx.pull.base_branch, entry.number);
        let pending = self.inner.pushes.begin(ctx.repo(), PushSource::User(user.id())).await;
        let lease = self.ctx().repositories.lease(entry.repo).await;
        let built = candidate(&self.merge_environment(), &spec, &target).await;
        drop(lease);
        match built {
            Ok(merged) => {
                self.ctx().database.merge_queue().testing(entry.id, &merged.commit, base_tip, Timestamp::now()).await?;
                self.inner.pushes.complete(pending).await;
                tracing::info!(number = entry.number, candidate = %merged.commit, "merge queue candidate built");
                Ok(())
            }
            Err(MergeError::Conflict(paths)) => self.leave(entry, "failed", &format!("the merge has conflicts in {}", paths.join(", "))).await,
            Err(error @ (MergeError::NothingToMerge | MergeError::Unrelated)) => self.leave(entry, "failed", &error.to_string()).await,
            Err(MergeError::BaseMoved) => Ok(()),
            Err(error) => Err(ServiceError::internal(error)),
        }
    }

    /// Lands `candidate_sha` on the base by compare-and-swap from `base_tip`, and records the
    /// pull request as merged by it. A base that moved means a rebuild on the next step.
    async fn land_entry(&self, user: &User, ctx: &PullContext, entry: &QueueEntry, candidate_sha: &str, base_tip: &str) -> Result<(), ServiceError> {
        let pending = self.inner.pushes.begin(ctx.repo(), PushSource::User(user.id())).await;
        let lease = self.ctx().repositories.lease(entry.repo).await;
        let landed = land(&self.merge_environment(), &ctx.repo().disk_path, &ctx.pull.base_branch, candidate_sha, base_tip).await;
        drop(lease);
        match landed {
            Ok(()) => {}
            Err(MergeError::BaseMoved) => {
                self.nudge(entry.repo, entry.base_branch.clone());
                return Ok(());
            }
            Err(error) => return Err(ServiceError::internal(error)),
        }
        let strategy = strategy_of(&entry.strategy);
        let record =
            MergeRecord { merged_by: Some(user.id()), strategy: Some(strategy), commit: candidate_sha, base: base_tip, head: &entry.head_sha, at: Timestamp::now() };
        if self.ctx().database.pulls().record_merge(ctx.issue().id, &record).await? {
            let event = TimelineEvent::Merged { commit: candidate_sha.to_owned(), strategy: Some(strategy) };
            self.inner.timeline.writer().close_as_system(ctx.issue().id, Some(user.id()), StateReason::Merged, event).await?;
        }
        self.inner.pushes.complete(pending).await;
        self.ctx().database.merge_queue().finish(entry.id, "landed", None, Timestamp::now()).await?;
        self.forget_candidate(ctx.repo(), entry).await;
        tracing::info!(number = entry.number, commit = %candidate_sha, "merge queue candidate landed");
        self.nudge(entry.repo, entry.base_branch.clone());
        Ok(())
    }

    /// Takes `entry` out of line as `state` for `reason`, and starts the next one.
    async fn leave(&self, entry: &QueueEntry, state: &str, reason: &str) -> Result<(), ServiceError> {
        if self.ctx().database.merge_queue().finish(entry.id, state, Some(reason), Timestamp::now()).await? {
            tracing::info!(number = entry.number, state, reason, "left the merge queue");
            if let Some(owned) = self.ctx().database.admin().find_repository(entry.repo).await? {
                let target = candidate_ref(&entry.base_branch, entry.number);
                if let Err(error) = drop_candidate(&self.merge_environment(), &self.ctx().repositories.path_for(owned.repo.owner_id, owned.repo.id), &target).await {
                    tracing::warn!(%error, target, "cannot delete a merge queue candidate");
                }
            }
            self.nudge(entry.repo, entry.base_branch.clone());
        }
        Ok(())
    }

    /// Deletes `entry`'s candidate ref; failures are logged.
    async fn forget_candidate(&self, repo: &RepoContext, entry: &QueueEntry) {
        let target = candidate_ref(&entry.base_branch, entry.number);
        if let Err(error) = drop_candidate(&self.merge_environment(), &repo.disk_path, &target).await {
            tracing::warn!(%error, target, "cannot delete a merge queue candidate");
        }
    }
}
