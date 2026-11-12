//! The merge box (requirements and their state) and merging.
//!
//! Milestone 2 workstream P (pull requests) owns this file.
//!
//! Merging needs `Write` on the base repository, which must not be archived
//! ([`RepoContext::ensure_writable`]), and every requirement of
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

use std::collections::HashSet;

use rbit_collaboration::pulls::{
    MergeDefaults, current_mergeability, merge_gate, merge_message, merges_as_writer, needs_trial_merge, still_expected,
};
use rbit_core::access::{Grants, repo_access};
use rbit_core::{
    AccountId, PushSource, StateReason, TimelineEvent, Timestamp, User,
    issue::MergeStrategy,
    protection::MergeRules,
    pull::{
        CommitIdentity, MergeFacts, Mergeability, Review, default_merge_title, default_squash_body, default_squash_title, evaluate,
        noreply_domain, tally,
    },
    status::{checks_status, required_checks},
};
use rbit_git::merge::{MergeError, MergeSpec, merge};
use rbit_storage::sqlite::{audit::AuditAction, pulls::MergeRecord};

use super::{PullContext, PullService, ReviewData, merge_lock};
use crate::{RepoContext, ServiceError, audit::AuditEntry, audit::AuditService};

mod contracts;
pub use contracts::{MergeBox, MergeFailure, MergeInput};

impl PullService {
    /// The merge box of `ctx` for `viewer`.
    pub async fn merge_box(&self, viewer: Option<&User>, ctx: &PullContext) -> Result<MergeBox, ServiceError> {
        self.merge_box_with(viewer, ctx, None).await
    }

    /// [`Self::merge_box`] for a page that already loaded the pull request's reviews
    /// ([`Self::reviews`]): the tally counts those instead of reading them again.
    pub async fn merge_box_from(&self, viewer: Option<&User>, ctx: &PullContext, reviews: &ReviewData) -> Result<MergeBox, ServiceError> {
        // Submitted reviews: the tally never counts pending ones.
        let reviews: Vec<Review> = reviews.reviews.iter().map(|entry| entry.review.clone()).collect();
        self.merge_box_with(viewer, ctx, Some(reviews)).await
    }

    /// The independent reads (the base tip, protection rules, the reviewers' access,
    /// statuses, the head branch, the viewer's rights, the commits for the squash
    /// message) run concurrently; their errors are reported in the order they used to be
    /// read in.
    async fn merge_box_with(
        &self,
        viewer: Option<&User>,
        ctx: &PullContext,
        reviews: Option<Vec<Review>>,
    ) -> Result<MergeBox, ServiceError> {
        let rules = async {
            match viewer {
                Some(user) if merges_as_writer(true, ctx.repo().access.can_write()) => {
                    self.inner.protection.check_merge(ctx.repo(), user, &ctx.pull.base_branch).await
                }
                Some(_) | None => Ok(MergeRules::unprotected()),
            }
        };
        let can_delete_head = async {
            match viewer {
                Some(user) => self.may_delete_head(user, ctx).await,
                None => Ok(false),
            }
        };
        let statuses = self.ctx().database.commit_statuses();
        let (base_tip, rules, voters, head_branch_exists, statuses, can_delete_head, (default_squash_title, default_squash_message)) = tokio::join!(
            self.base_tip(ctx),
            rules,
            self.voters(ctx, reviews),
            self.head_branch_exists(ctx),
            statuses.latest(ctx.repo().repo.repo.id, &ctx.pull.head_sha),
            can_delete_head,
            self.default_squash_message(ctx),
        );
        let base_tip = base_tip?;
        let rules = rules?;
        let required = self.required_for_head(ctx, &rules.required_contexts).await?;
        let (reviews, writers) = voters?;
        let tally = tally(&reviews, ctx.issue().author, &ctx.pull.head_sha, rules.dismiss_stale_approvals, |id| writers.contains(&id));
        let statuses = statuses?;
        let computed_on = (ctx.pull.mergeable_base_sha.as_deref(), ctx.pull.mergeable_head_sha.as_deref());
        let mergeable =
            current_mergeability(ctx.pull.mergeable, computed_on, base_tip.as_deref(), &ctx.pull.head_sha, ctx.issue().is_open());
        if needs_trial_merge(mergeable, ctx.issue().is_open(), ctx.pull.is_draft) {
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
            required_checks: required_checks(&required, &statuses),
        };
        let evaluation = evaluate(&facts, &rules);
        let can_delete_head = can_delete_head?;
        let (default_title, default_message) = Self::default_merge_message(ctx);
        Ok(MergeBox {
            evaluation,
            rules,
            tally,
            mergeable,
            conflict_paths: if mergeable == Mergeability::Conflict { ctx.pull.conflict_paths.clone() } else { Vec::new() },
            statuses,
            required,
            head_branch_exists,
            can_merge: merges_as_writer(viewer.is_some(), ctx.repo().can_write_content()),
            can_delete_head,
            default_title,
            default_message,
            default_squash_title,
            default_squash_message,
        })
    }

    /// The pull request's reviews (`reviews`, or read here) and which of their reviewers
    /// currently have write access: what [`tally`] counts.
    async fn voters(&self, ctx: &PullContext, reviews: Option<Vec<Review>>) -> Result<(Vec<Review>, HashSet<AccountId>), ServiceError> {
        let reviews = match reviews {
            Some(reviews) => reviews,
            None => self.ctx().database.reviews().reviews(ctx.issue().id).await?,
        };
        let reviewers: Vec<_> = reviews.iter().filter_map(|review| review.reviewer).collect();
        let users = self.ctx().database.accounts().find_users(&reviewers).await?;
        let repo = &ctx.repo().repo;
        let need_grants: Vec<_> = users.iter().filter(|user| !user.is_admin && user.id() != repo.repo.owner_id).map(User::id).collect();
        let grants = self.ctx().database.repositories().grants_for(repo.repo.id, repo.repo.owner_id, &need_grants).await?;
        let mut writers = HashSet::new();
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
        Ok((reviews, writers))
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
        ctx.repo().ensure_writable()?;
        let lock = merge_lock(&self.inner.merge_locks, ctx.repo().repo.repo.id);
        let _serialised = lock.lock().await;
        let _slot = self.inner.merge_slots.acquire().await.map_err(ServiceError::internal)?;

        // Re-read under the lock: another merge may just have finished.
        let threads = self.inner.timeline.reader();
        let thread = threads.open(Some(actor), ctx.repo().clone(), ctx.issue().number).await?;
        let ctx = self.context(thread).await?;
        if !still_expected(input.expected_head.as_deref(), &ctx.pull.head_sha) {
            return Err(MergeFailure::HeadChanged);
        }
        if !still_expected(input.expected_base.as_deref(), &ctx.pull.base_branch) {
            return Err(MergeFailure::BaseChanged);
        }
        if ctx.pull.is_merged() || !ctx.issue().is_open() {
            return Err(MergeFailure::Blocked("the pull request is closed".to_owned()));
        }
        let mut merge_box = self.merge_box(Some(actor), &ctx).await?;
        if needs_trial_merge(merge_box.mergeable, true, ctx.pull.is_draft) {
            // Not computed yet for this base: compute it now rather than make the merger wait.
            let base_tip = self.base_tip(&ctx).await?.ok_or_else(|| MergeFailure::Blocked("the base branch does not exist".to_owned()))?;
            let (state, paths) = self.trial_merge(ctx.repo(), &base_tip, &ctx.pull.head_sha).await?;
            self.ctx().database.pulls().set_mergeability(ctx.issue().id, state, &base_tip, &ctx.pull.head_sha, &paths).await?;
            let refreshed = self.context(threads.open(Some(actor), ctx.repo().clone(), ctx.issue().number).await?).await?;
            merge_box = self.merge_box(Some(actor), &refreshed).await?;
        }
        if merge_gate(&merge_box.evaluation, input.bypass).map_err(MergeFailure::Blocked)? {
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
        let defaults = MergeDefaults {
            merge: (&merge_box.default_title, &merge_box.default_message),
            squash: (&merge_box.default_squash_title, &merge_box.default_squash_message),
        };
        merge_message(input.strategy, &input.title, &input.message, defaults)
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
        let pending = self.inner.pushes.begin(repo, PushSource::User(actor.id())).await;
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
            self.inner.timeline.writer().close_as_system(ctx.issue().id, Some(actor.id()), StateReason::Merged, event).await?;
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
            if !head_ctx.can_write_content() || !self.inner.protection.may_delete_branch(&head_ctx, actor, &ctx.pull.head_branch).await? {
                return Ok(false);
            }
            let pending = self.inner.pushes.begin(&head_ctx, PushSource::User(actor.id())).await;
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

    /// Whether `actor` may delete the head branch: write access to the unarchived head repository and
    /// no protection rule against deleting it (`ProtectionService::may_delete_branch`).
    ///
    /// A head branch in the base repository uses the access this request already computed
    /// for `actor` there; a fork's head opens the fork for them.
    pub async fn may_delete_head(&self, actor: &User, ctx: &PullContext) -> Result<bool, ServiceError> {
        let Some(head) = &ctx.head_repo else { return Ok(false) };
        let opened;
        let head_ctx = if head.repo.id == ctx.repo().repo.repo.id && ctx.repo().viewer == Some(actor.id()) {
            ctx.repo()
        } else {
            opened = match self.inner.repositories.open(Some(actor), head.owner.name.as_str(), head.repo.name.as_str()).await {
                Ok(head_ctx) => head_ctx,
                Err(ServiceError::NotFound) => return Ok(false),
                Err(error) => return Err(error),
            };
            &opened
        };
        Ok(head_ctx.can_write_content() && self.inner.protection.may_delete_branch(head_ctx, actor, &ctx.pull.head_branch).await?)
    }

    // Agent loop: checks a commit does not need.
    /// The `required` contexts the head commit needs: a required check that rbit Actions
    /// reports (`<workflow> / <job> (<event>)`) is not required of a commit for which the
    /// trigger recorded that every workflow left it out (`branches` or `paths` filters, a
    /// skip marker), or a docs-only push would never become mergeable.
    async fn required_for_head<S: ToString>(&self, ctx: &PullContext, required: &[S]) -> Result<Vec<String>, ServiceError> {
        let required: Vec<String> = required.iter().map(ToString::to_string).collect();
        let decision = self.ctx().database.actions_runs().decision(ctx.repo().repo.repo.id, &ctx.pull.head_sha).await?;
        if !decision.is_some_and(|decision| decision.started == 0 && matches!(decision.reason.as_str(), "filtered" | "skip_marker")) {
            return Ok(required);
        }
        Ok(required.into_iter().filter(|context| !is_actions_context(context)).collect())
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
        if head.repo.is_archived() {
            return Err(ServiceError::Archived);
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

/// Whether `context` is the shape of an rbit Actions job status: `<workflow> / <job> (<event>)`.
fn is_actions_context(context: &str) -> bool {
    context.contains(" / ") && context.ends_with(')') && context.contains(" (")
}
