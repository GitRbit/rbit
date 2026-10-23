//! The merge box (requirements and their state) and merging.
//!
//! Milestone 2 workstream P (pull requests) owns this file.
//!
//! Merging needs `Write` on the base repository and every requirement of
//! [`rbit_core::pull::evaluate`]; protection rules come from
//! [`crate::ProtectionService::check_merge`]. A merger whose rules allow a bypass may merge
//! anyway after confirming (`MergeInput::bypass`), which is audited as
//! `BranchProtectionBypassed`. Merges are serialised per base repository and bounded
//! instance-wide (`[pulls] max_concurrent_merges`); the git work is `rbit_git::merge`,
//! wrapped in [`crate::PushService`] so `RepositoryPushed` (webhooks, commit references)
//! fires as for any push. A base branch that moves during the merge is retried once.
//!
//! Checks (milestone 3, S): the merge box reads the latest commit statuses of the head in
//! the base repository; with `require_status_checks`, every required context must be
//! `success` (a protection requirement, so an allowed bypass skips it). Other statuses
//! are shown but never block.

use rbit_auth::policy::{Grants, repo_access};
use rbit_core::{
    StateReason, TimelineEvent, Timestamp, User,
    issue::MergeStrategy,
    protection::MergeRules,
    pull::{
        CommitIdentity, MergeEvaluation, MergeFacts, Mergeability, ReviewTally, commit_message, default_merge_title, default_squash_body,
        default_squash_title, evaluate, noreply_domain, tally,
    },
    status::{CommitStatus, checks_status, required_checks},
};
use rbit_git::merge::{MergeError, MergeSpec, merge};
use rbit_storage::sqlite::{audit::AuditAction, pulls::MergeRecord};

use super::{PullContext, PullService, merge_lock};
use crate::{RepoContext, ServiceError, audit::AuditEntry, audit::AuditService};

/// The merge box as one viewer sees it.
#[derive(Debug, Clone)]
pub struct MergeBox {
    pub evaluation: MergeEvaluation,
    pub rules: MergeRules,
    pub tally: ReviewTally,
    /// The mergeability shown: the stored one, or `Unknown` when it ran on another base.
    pub mergeable: Mergeability,
    pub conflict_paths: Vec<String>,
    /// The latest status of each context on the head commit, by context (milestone 3, S).
    pub statuses: Vec<CommitStatus>,
    pub head_branch_exists: bool,
    /// The viewer may press the button (write access to the base repository).
    pub can_merge: bool,
    /// The viewer may delete the head branch after merging.
    pub can_delete_head: bool,
    pub default_title: String,
    pub default_message: String,
    pub default_squash_title: String,
    pub default_squash_message: String,
}

/// A merge as requested.
#[derive(Debug, Clone)]
pub struct MergeInput {
    pub strategy: MergeStrategy,
    /// Empty: the default for the strategy.
    pub title: String,
    pub message: String,
    /// The head the merger saw; a different current head refuses the merge.
    pub expected_head: Option<String>,
    /// The base branch the merger saw; a different current base refuses the merge.
    pub expected_base: Option<String>,
    pub delete_head_branch: bool,
    /// Merge despite unmet protection rules (only for mergers allowed to bypass).
    pub bypass: bool,
}

/// Why a merge did not happen.
#[derive(Debug, thiserror::Error)]
pub enum MergeFailure {
    /// The head is not the one the merger reviewed (`409`).
    #[error("the head branch was modified; review the new changes and try again")]
    HeadChanged,
    /// The base branch is not the one the merger saw (`409`).
    #[error("the base branch was changed; review the pull request and try again")]
    BaseChanged,
    /// A requirement is unmet (`405`); the message says which.
    #[error("{0}")]
    Blocked(String),
    #[error("{0}")]
    Service(#[from] ServiceError),
}

impl From<rbit_storage::StorageError> for MergeFailure {
    fn from(error: rbit_storage::StorageError) -> Self {
        Self::Service(error.into())
    }
}

impl PullService {
    /// The merge box of `ctx` for `viewer`.
    pub async fn merge_box(&self, viewer: Option<&User>, ctx: &PullContext) -> Result<MergeBox, ServiceError> {
        let base_tip = self.base_tip(ctx).await?;
        let rules = match viewer {
            Some(user) if ctx.repo().access.can_write() => {
                self.inner.protection.check_merge(ctx.repo(), user, &ctx.pull.base_branch).await?
            }
            Some(_) | None => MergeRules::unprotected(),
        };
        let tally = self.tally(ctx, &rules).await?;
        let head_branch_exists = self.head_branch_exists(ctx).await;
        let statuses = self.ctx().database.commit_statuses().latest(ctx.repo().repo.repo.id, &ctx.pull.head_sha).await?;
        let stale = base_tip.is_none()
            || ctx.pull.mergeable_base_sha != base_tip
            || ctx.pull.mergeable_head_sha.as_deref() != Some(&ctx.pull.head_sha);
        let mergeable = if stale && ctx.issue().is_open() { Mergeability::Unknown } else { ctx.pull.mergeable };
        if mergeable == Mergeability::Unknown && ctx.issue().is_open() && !ctx.pull.is_draft {
            if ctx.pull.mergeable != Mergeability::Unknown {
                self.ctx().database.pulls().mark_unknown(ctx.issue().id).await?;
            }
            self.inner.wake.notify_one();
        }
        let facts = MergeFacts {
            is_open: ctx.issue().is_open(),
            is_draft: ctx.pull.is_draft,
            head_branch_exists,
            mergeable,
            tally: &tally,
            checks: checks_status(&statuses),
            required_checks: required_checks(&rules.required_contexts, &statuses),
        };
        let evaluation = evaluate(&facts, &rules);
        let can_delete_head = match viewer {
            Some(user) => self.may_delete_head(user, ctx).await?,
            None => false,
        };
        let (default_title, default_message) = Self::default_merge_message(ctx);
        let (default_squash_title, default_squash_message) = self.default_squash_message(ctx).await;
        Ok(MergeBox {
            evaluation,
            rules,
            tally,
            mergeable,
            conflict_paths: if mergeable == Mergeability::Conflict { ctx.pull.conflict_paths.clone() } else { Vec::new() },
            statuses,
            head_branch_exists,
            can_merge: viewer.is_some() && ctx.repo().access.can_write(),
            can_delete_head,
            default_title,
            default_message,
            default_squash_title,
            default_squash_message,
        })
    }

    /// Reviews counted for `rules`: latest decisive review per current writer.
    pub(super) async fn tally(&self, ctx: &PullContext, rules: &MergeRules) -> Result<ReviewTally, ServiceError> {
        let reviews = self.ctx().database.reviews().reviews(ctx.issue().id).await?;
        let reviewers: Vec<_> = reviews.iter().filter_map(|review| review.reviewer).collect();
        let users = self.ctx().database.accounts().find_users(&reviewers).await?;
        let repo = &ctx.repo().repo;
        let need_grants: Vec<_> = users.iter().filter(|user| !user.is_admin && user.id() != repo.repo.owner_id).map(User::id).collect();
        let grants = self.ctx().database.repositories().grants_for(repo.repo.id, repo.repo.owner_id, &need_grants).await?;
        let mut writers = std::collections::HashSet::new();
        for user in users {
            let stored = grants.get(&user.id()).copied().unwrap_or_default();
            let grants = Grants {
                org_role: stored.org_role,
                org_base: stored.org_base.permission(),
                team: stored.team,
                collaborator: stored.collaborator,
            };
            if repo_access(Some(&user), repo, &grants).can_write() {
                writers.insert(user.id());
            }
        }
        Ok(tally(&reviews, ctx.issue().author, &ctx.pull.head_sha, rules.dismiss_stale_approvals, |id| writers.contains(&id)))
    }

    fn default_merge_message(ctx: &PullContext) -> (String, String) {
        (default_merge_title(ctx.issue().number, &ctx.head_owner_branch()), ctx.issue().title.to_string())
    }

    async fn default_squash_message(&self, ctx: &PullContext) -> (String, String) {
        let commits = self.commits(ctx).await.unwrap_or_default();
        let body = default_squash_body(commits.iter().rev().map(|commit| commit.summary.as_str()));
        (default_squash_title(ctx.issue().title.as_str(), ctx.issue().number), body)
    }

    /// Merges the pull request as `actor` (see the module docs). Returns the new tip of
    /// the base branch.
    pub async fn merge(&self, actor: &User, ctx: &PullContext, input: MergeInput) -> Result<String, MergeFailure> {
        if !ctx.repo().access.can_write() {
            return Err(ServiceError::Forbidden.into());
        }
        let lock = merge_lock(&self.inner.merge_locks, ctx.repo().repo.repo.id);
        let _serialised = lock.lock().await;
        let _slot = self.inner.merge_slots.acquire().await.map_err(ServiceError::internal)?;

        // Re-read under the lock: another merge may just have finished.
        let thread = self.inner.timeline.open(Some(actor), ctx.repo().clone(), ctx.issue().number).await?;
        let ctx = self.context(thread).await?;
        if let Some(expected) = input.expected_head.as_deref().filter(|expected| !expected.is_empty())
            && expected != ctx.pull.head_sha
        {
            return Err(MergeFailure::HeadChanged);
        }
        if let Some(expected) = input.expected_base.as_deref().filter(|expected| !expected.is_empty())
            && expected != ctx.pull.base_branch
        {
            return Err(MergeFailure::BaseChanged);
        }
        if ctx.pull.is_merged() || !ctx.issue().is_open() {
            return Err(MergeFailure::Blocked("the pull request is closed".to_owned()));
        }
        let mut merge_box = self.merge_box(Some(actor), &ctx).await?;
        if merge_box.mergeable == Mergeability::Unknown && !ctx.pull.is_draft {
            // Not computed yet for this base: compute it now rather than make the merger wait.
            let base_tip = self.base_tip(&ctx).await?.ok_or_else(|| MergeFailure::Blocked("the base branch does not exist".to_owned()))?;
            let (state, paths) = self.trial_merge(ctx.repo(), &base_tip, &ctx.pull.head_sha).await?;
            self.ctx().database.pulls().set_mergeability(ctx.issue().id, state, &base_tip, &ctx.pull.head_sha, &paths).await?;
            let refreshed = self.context(self.inner.timeline.open(Some(actor), ctx.repo().clone(), ctx.issue().number).await?).await?;
            merge_box = self.merge_box(Some(actor), &refreshed).await?;
        }
        let evaluation = &merge_box.evaluation;
        if !evaluation.can_merge() {
            if !(input.bypass && evaluation.can_merge_with_bypass()) {
                let unmet =
                    evaluation.checks.iter().find(|check| !check.met).map_or_else(String::new, |check| check.requirement.describe(false));
                return Err(MergeFailure::Blocked(format!("this pull request cannot be merged: {unmet}")));
            }
            AuditService::from_store(self.ctx().database.audit())
                .record(AuditEntry::new(AuditAction::BranchProtectionBypassed).by(actor).on(ctx.repo().repo.full_name()).detail(format!(
                    "merged pull request #{} into {}",
                    ctx.issue().number.get(),
                    ctx.pull.base_branch
                )))
                .await;
        }

        let message = Self::message_for(&input, &merge_box);
        let commit = self.run_merge(actor, &ctx, input.strategy, &message).await?;
        if input.delete_head_branch && merge_box.can_delete_head {
            self.delete_head_branch(actor, &ctx).await;
        }
        Ok(commit)
    }

    fn message_for(input: &MergeInput, merge_box: &MergeBox) -> String {
        let (title, body) = match input.strategy {
            MergeStrategy::Merge => (&merge_box.default_title, &merge_box.default_message),
            MergeStrategy::Squash | MergeStrategy::Rebase => (&merge_box.default_squash_title, &merge_box.default_squash_message),
        };
        let title = if input.title.trim().is_empty() { title.as_str() } else { input.title.trim() };
        let body = if input.title.trim().is_empty() && input.message.trim().is_empty() { body.as_str() } else { input.message.as_str() };
        commit_message(title, body)
    }

    /// The git part: merge, record, close, report the push. Retries once if the base moved.
    async fn run_merge(&self, actor: &User, ctx: &PullContext, strategy: MergeStrategy, message: &str) -> Result<String, MergeFailure> {
        let config = &self.ctx().config;
        let domain = noreply_domain(&config.pulls.noreply_domain, &config.server.public_url);
        let merger = CommitIdentity::noreply(actor.name().as_str(), actor.account.display_name.as_deref(), &domain);
        let author = match ctx.thread.author.as_ref() {
            Some(author) => CommitIdentity::noreply(author.name.as_str(), author.display_name.as_deref(), &domain),
            None => CommitIdentity::noreply("ghost", None, &domain),
        };
        let env = self.merge_environment();
        let repo: &RepoContext = ctx.repo();
        let pending = self.inner.pushes.begin(repo, Some(actor.id())).await;
        let lease = self.ctx().repositories.lease(repo.repo.repo.id).await;
        let mut attempt = 0;
        let (commit, base_tip) = loop {
            attempt += 1;
            let base_tip = self.base_tip(ctx).await?.ok_or_else(|| MergeFailure::Blocked("the base branch does not exist".to_owned()))?;
            let spec = MergeSpec {
                repo: &repo.disk_path,
                base_branch: &ctx.pull.base_branch,
                base_tip: &base_tip,
                head: &ctx.pull.head_sha,
                strategy,
                message,
                author: &author,
                merger: &merger,
                at: Timestamp::now(),
            };
            match merge(&env, &spec).await {
                Ok(result) => break (result.commit, base_tip),
                Err(MergeError::BaseMoved) if attempt == 1 => {}
                Err(MergeError::BaseMoved) => return Err(MergeFailure::Blocked("the base branch changed; try again".to_owned())),
                Err(MergeError::Conflict(paths)) => {
                    self.ctx()
                        .database
                        .pulls()
                        .set_mergeability(ctx.issue().id, Mergeability::Conflict, &base_tip, &ctx.pull.head_sha, &paths)
                        .await?;
                    return Err(MergeFailure::Blocked(format!("the merge has conflicts in {}", paths.join(", "))));
                }
                Err(error @ (MergeError::NothingToMerge | MergeError::Unrelated)) => return Err(MergeFailure::Blocked(error.to_string())),
                Err(error) => return Err(ServiceError::internal(error).into()),
            }
        };
        drop(lease);

        // Record before the push is reported, so the base-push handler sees it merged.
        let record = MergeRecord {
            merged_by: Some(actor.id()),
            strategy: Some(strategy),
            commit: &commit,
            base: &base_tip,
            head: &ctx.pull.head_sha,
            at: Timestamp::now(),
        };
        if self.ctx().database.pulls().record_merge(ctx.issue().id, &record).await? {
            let event = TimelineEvent::Merged { commit: commit.clone(), strategy: Some(strategy) };
            self.inner.timeline.close_as_system(ctx.issue().id, Some(actor.id()), StateReason::Merged, event).await?;
        }
        self.inner.pushes.complete(pending).await;
        tracing::info!(actor = %actor.name(), repo = %repo.repo.full_name(), number = ctx.issue().number.get(), %strategy, "pull request merged");
        Ok(commit)
    }

    /// Deletes the merged head branch if protection allows and it still points at the
    /// merged head; reported through `PushService` of the head repository. Failures are
    /// logged: the merge itself succeeded.
    async fn delete_head_branch(&self, actor: &User, ctx: &PullContext) {
        let Some(head) = &ctx.head_repo else { return };
        let result = async {
            let head_ctx = self.inner.repositories.open(Some(actor), head.owner.name.as_str(), head.repo.name.as_str()).await?;
            if !head_ctx.access.can_write() || !self.inner.protection.may_delete_branch(&head_ctx, actor, &ctx.pull.head_branch).await? {
                return Ok(false);
            }
            let pending = self.inner.pushes.begin(&head_ctx, Some(actor.id())).await;
            let lease = self.ctx().repositories.lease(head_ctx.repo.repo.id).await;
            let deleted =
                rbit_git::pull_refs::delete_branch(self.git_binary(), &head_ctx.disk_path, &ctx.pull.head_branch, &ctx.pull.head_sha)
                    .await?;
            drop(lease);
            if deleted {
                self.inner.pushes.complete(pending).await;
            }
            Ok::<_, ServiceError>(deleted)
        };
        match result.await {
            Ok(true) => tracing::info!(branch = %ctx.pull.head_branch, "deleted the merged head branch"),
            Ok(false) => tracing::info!(branch = %ctx.pull.head_branch, "kept the head branch (protected, moved or not writable)"),
            Err(error) => tracing::warn!(%error, branch = %ctx.pull.head_branch, "cannot delete the merged head branch"),
        }
    }

    /// Whether `actor` may delete the head branch: write access to the head repository and
    /// no protection rule against deleting it (`ProtectionService::may_delete_branch`).
    pub async fn may_delete_head(&self, actor: &User, ctx: &PullContext) -> Result<bool, ServiceError> {
        let Some(head) = &ctx.head_repo else { return Ok(false) };
        let head_ctx = match self.inner.repositories.open(Some(actor), head.owner.name.as_str(), head.repo.name.as_str()).await {
            Ok(head_ctx) => head_ctx,
            Err(ServiceError::NotFound) => return Ok(false),
            Err(error) => return Err(error),
        };
        Ok(head_ctx.access.can_write() && self.inner.protection.may_delete_branch(&head_ctx, actor, &ctx.pull.head_branch).await?)
    }

    /// Deletes the head branch of a merged or closed pull request on request (the "Delete
    /// branch" button). `false` when it was not deleted.
    pub async fn delete_head(&self, actor: &User, ctx: &PullContext) -> Result<bool, ServiceError> {
        if ctx.issue().is_open() {
            return Err(ServiceError::Invalid("close or merge the pull request first".to_owned()));
        }
        let Some(head) = &ctx.head_repo else { return Ok(false) };
        if !self.access_of(actor.id(), head).await?.can_write() {
            return Err(ServiceError::Forbidden);
        }
        if !self.may_delete_head(actor, ctx).await? {
            return Err(ServiceError::Invalid("the head branch is protected".to_owned()));
        }
        if !self.head_branch_exists(ctx).await {
            return Ok(false);
        }
        self.delete_head_branch(actor, ctx).await;
        Ok(!self.head_branch_exists(ctx).await)
    }
}
