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
//! * A required context that cannot pass on the candidate takes the entry out of line rather
//!   than holding it up for ever: one rbit's Actions report only for another event (such as
//!   `CI / test (pull_request)`), one of its push runs that none of the candidate's finished
//!   runs reported, and any that has not passed [`stall::CHECK_TIMEOUT`] after the candidate
//!   was built, as GitHub's merge queue times its checks out.
//!
//! The queue advances on commit statuses and pushes of its repository, and on a sweep every
//! [`QUEUE_SWEEP`] in case an event was missed.

mod landing;
mod stall;

use std::time::Duration;

use rbit_core::{
    IssueNumber, PushSource, RepoId, StateReason, TimelineEvent, Timestamp, User,
    issue::MergeStrategy,
    pull::{CommitIdentity, Mergeability, Requirement, noreply_domain},
    status::StatusState,
};
use rbit_git::merge::{
    MergeError, MergeSpec,
    candidate::{candidate, candidate_ref, drop_candidate, land},
};
use rbit_storage::sqlite::{
    actions_runs::{NewEntry, QueueEntry},
    pulls::MergeRecord,
};

use super::{MergeBox, MergeFailure, PullContext, PullService, merge_lock};
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

/// What a pull request still lacks to be queued or to land, but its checks: the queue tests
/// those itself, and a conflict check still running is no reason to wait (building the
/// candidate finds a conflict).
fn unmet_requirements(merge_box: &MergeBox) -> Vec<String> {
    merge_box
        .evaluation
        .checks
        .iter()
        .filter(|check| {
            !check.met
                && !matches!(
                    check.requirement,
                    Requirement::RequiredChecks(_) | Requirement::Checks(_) | Requirement::NoConflicts(Mergeability::Unknown)
                )
        })
        .map(|check| check.requirement.describe(false))
        .collect()
}

impl PullService {
    /// Queues `ctx` to land with `strategy` for head `expected_head` (the current one when
    /// `None`). Every requirement but its checks must be met; a conflict check still running
    /// is no reason to wait, as building the candidate finds a conflict. Queuing a pull
    /// request that is in line already reports its place.
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
        let unmet = unmet_requirements(&self.merge_box(Some(actor), ctx).await?);
        if !unmet.is_empty() {
            return Err(MergeFailure::Blocked(unmet.join("; ")));
        }
        let store = self.ctx().database.actions_runs();
        let new = NewEntry {
            repo: ctx.repo().repo.repo.id,
            issue: ctx.issue().id,
            number: ctx.issue().number.get(),
            base_branch: &ctx.pull.base_branch,
            head_sha: &ctx.pull.head_sha,
            strategy: strategy_name(strategy),
            queued_by: actor.id(),
        };
        if store.queue_pull(&new, Timestamp::now()).await?.is_some() {
            tracing::info!(repo = %ctx.repo().repo.full_name(), number = ctx.issue().number.get(), "pull request queued to land");
            self.nudge(ctx.repo().repo.repo.id, ctx.pull.base_branch.clone());
        }
        Ok(self.queue_place(ctx).await?.ok_or(ServiceError::NotFound)?)
    }

    /// Where `ctx` stands in its line: its newest entry, live or not.
    pub async fn queue_place(&self, ctx: &PullContext) -> Result<Option<QueuePlace>, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let Some(entry) = store.queue_of_pull(ctx.issue().id).await? else { return Ok(None) };
        let line = store.queue_line(entry.repo, &entry.base_branch).await?;
        let position = line.iter().position(|live| live.id == entry.id).map(|index| index + 1);
        Ok(Some(QueuePlace { entry, position }))
    }

    /// Takes `ctx` out of line; `false` when it was not in line.
    pub async fn dequeue(&self, actor: &User, ctx: &PullContext) -> Result<bool, ServiceError> {
        // Taking an entry out writes nothing to the repository: it works when archived too.
        if !ctx.repo().access.can_write() {
            return Err(ServiceError::Forbidden);
        }
        let store = self.ctx().database.actions_runs();
        let Some(entry) = store.queue_of_pull(ctx.issue().id).await? else { return Ok(false) };
        // Under the line's lock, as `advance`: an entry being landed or built lands or is
        // built first, and one taken out is never landed afterwards.
        let lock = merge_lock(&self.inner.merge_locks, entry.repo);
        let guard = lock.lock().await;
        let removed =
            store.queue_finish(entry.id, "removed", Some(&format!("taken out of line by {}", actor.name())), Timestamp::now()).await?;
        if removed {
            self.forget_candidate(ctx.repo(), &entry).await;
        }
        drop(guard);
        if removed {
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
        let lines = self.ctx().database.actions_runs().queue_lines().await;
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
            for (repo, base) in self.ctx().database.actions_runs().queue_lines().await.unwrap_or_default() {
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
        let store = self.ctx().database.actions_runs();
        let Some(entry) = store.queue_line(repo, base).await?.into_iter().next() else { return Ok(()) };
        let database = &self.ctx().database;
        let Some(user) = database.accounts().find_active_user(entry.queued_by).await? else {
            return self.leave(&entry, "failed", "whoever queued it is gone or suspended").await;
        };
        let Some(owned) = database.admin().find_repository(repo).await? else { return Ok(()) };
        let opened = self.inner.repositories.open(Some(&user), owned.owner.name.as_str(), owned.repo.name.as_str()).await;
        let repo_ctx = match opened {
            Ok(repo_ctx) => repo_ctx,
            // An entry that cannot be looked at would hold up the line for ever.
            Err(ServiceError::NotFound | ServiceError::Forbidden) => {
                return self.leave(&entry, "failed", "whoever queued it can no longer read the repository").await;
            }
            Err(error) => return Err(error),
        };
        let number = IssueNumber::new(entry.number).ok_or(ServiceError::NotFound)?;
        let ctx = self.open(Some(&user), repo_ctx, number).await?;
        if !ctx.issue().is_open() {
            return self.leave(&entry, "removed", "the pull request is no longer open").await;
        }
        if !ctx.repo().can_write_content() {
            return self.leave(&entry, "failed", "the repository is archived, or whoever queued it may no longer write to it").await;
        }
        // What held when it was queued is checked again before each build and landing.
        let merge_box = self.merge_box(Some(&user), &ctx).await?;
        let unmet = unmet_requirements(&merge_box);
        if !unmet.is_empty() {
            return self.leave(&entry, "failed", &format!("no longer ready: {}", unmet.join("; "))).await;
        }
        if ctx.pull.head_sha != entry.head_sha {
            return self.leave(&entry, "failed", "the head moved after it was queued; queue it again").await;
        }
        // Its candidate ref is named by the base it was queued for; a retargeted pull
        // request belongs in another line.
        if ctx.pull.base_branch != entry.base_branch {
            let reason = format!("its base changed to `{}` after it was queued; queue it again", ctx.pull.base_branch);
            return self.leave(&entry, "failed", &reason).await;
        }
        let Some(base_tip) = self.base_tip(&ctx).await? else {
            return self.leave(&entry, "failed", "the base branch does not exist").await;
        };
        match (entry.state.as_str(), entry.candidate_sha.as_deref(), entry.base_sha.as_deref()) {
            ("testing", Some(candidate_sha), Some(built_on)) if built_on == base_tip => {
                let rules = merge_box.rules;
                let statuses = database.commit_statuses().latest(repo, candidate_sha).await?;
                let (mut waiting, mut missing) = (Vec::new(), Vec::new());
                for context in &rules.required_contexts {
                    match statuses.iter().find(|status| status.context.as_str() == context.as_str()).map(|status| status.state) {
                        Some(StatusState::Success) => {}
                        Some(StatusState::Failure | StatusState::Error) => {
                            let reason = format!("`{context}` failed on the candidate {}", &candidate_sha[..candidate_sha.len().min(12)]);
                            return self.leave(&entry, "failed", &reason).await;
                        }
                        Some(StatusState::Pending) => waiting.push(context.as_str()),
                        None => {
                            waiting.push(context.as_str());
                            missing.push(context.as_str());
                        }
                    }
                }
                if waiting.is_empty() {
                    return self.land_entry(&user, &ctx, &entry, candidate_sha, &base_tip).await;
                }
                match self.stalled(&entry, candidate_sha, &waiting, &missing).await? {
                    Some(reason) => self.leave(&entry, "failed", &reason).await,
                    None => Ok(()),
                }
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
            Ok(built) => {
                self.ctx().database.actions_runs().queue_testing(entry.id, &built.commit, base_tip, Timestamp::now()).await?;
                self.inner.pushes.complete(pending).await;
                tracing::info!(number = entry.number, candidate = %built.commit, "merge queue candidate built");
                Ok(())
            }
            Err(MergeError::Conflict(paths)) => {
                self.leave(entry, "failed", &format!("the merge has conflicts in {}", paths.join(", "))).await
            }
            Err(error @ (MergeError::NothingToMerge | MergeError::Unrelated)) => self.leave(entry, "failed", &error.to_string()).await,
            Err(MergeError::BaseMoved) => Ok(()),
            Err(error) => Err(ServiceError::internal(error)),
        }
    }

    /// Takes `entry` out of line as `state` for `reason`, and starts the next one.
    async fn leave(&self, entry: &QueueEntry, state: &str, reason: &str) -> Result<(), ServiceError> {
        if self.ctx().database.actions_runs().queue_finish(entry.id, state, Some(reason), Timestamp::now()).await? {
            tracing::info!(number = entry.number, state, reason, "left the merge queue");
            if let Some(owned) = self.ctx().database.admin().find_repository(entry.repo).await? {
                let target = candidate_ref(&entry.base_branch, entry.number);
                if let Err(error) = drop_candidate(
                    &self.merge_environment(),
                    &self.ctx().repositories.path_for(owned.repo.owner_id, owned.repo.id),
                    &target,
                )
                .await
                {
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
