//! Final queue landing: hold the repository in place and recheck the queuer.

use super::{
    MergeError, MergeRecord, PullContext, PullService, PushSource, QueueEntry, ServiceError, StateReason, TimelineEvent, Timestamp, User,
    land, strategy_of,
};

impl PullService {
    /// Lands `candidate_sha` on the base by compare-and-swap from `base_tip`, and records the
    /// pull request as merged by it. A base that moved means a rebuild on the next step.
    pub(super) async fn land_entry(
        &self,
        user: &User,
        ctx: &PullContext,
        entry: &QueueEntry,
        candidate_sha: &str,
        base_tip: &str,
    ) -> Result<(), ServiceError> {
        self.land_entry_after(user, ctx, entry, (candidate_sha, base_tip), std::future::ready(())).await
    }

    // Private static seam: production passes Ready, while tests suspend the
    // captured principal after the lease and immediately before authorization.
    async fn land_entry_after(
        &self,
        user: &User,
        ctx: &PullContext,
        entry: &QueueEntry,
        commits: (&str, &str),
        after_lease: impl std::future::Future<Output = ()> + Send,
    ) -> Result<(), ServiceError> {
        let (candidate_sha, base_tip) = commits;
        let pending = self.inner.pushes.begin(ctx.repo(), PushSource::User(user.id())).await;
        let lease = self.ctx().repositories.lease(entry.repo).await;
        after_lease.await;
        // The earlier queue evaluation awaited Git and SQL work. Do not let its
        // User snapshot authorize this write after the account was suspended.
        if self.ctx().database.accounts().find_active_user(entry.queued_by).await?.is_none() {
            drop(lease);
            return self.leave(entry, "failed", "whoever queued it is gone or suspended").await;
        }
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
        let record = MergeRecord {
            merged_by: Some(user.id()),
            strategy: Some(strategy),
            commit: candidate_sha,
            base: base_tip,
            head: &entry.head_sha,
            at: Timestamp::now(),
        };
        if self.ctx().database.pulls().record_merge(ctx.issue().id, &record).await? {
            let event = TimelineEvent::Merged { commit: candidate_sha.to_owned(), strategy: Some(strategy) };
            self.inner.timeline.writer().close_as_system(ctx.issue().id, Some(user.id()), StateReason::Merged, event).await?;
        }
        self.inner.pushes.complete(pending).await;
        self.ctx().database.actions_runs().queue_finish(entry.id, "landed", None, Timestamp::now()).await?;
        self.forget_candidate(ctx.repo(), entry).await;
        tracing::info!(number = entry.number, commit = %candidate_sha, "merge queue candidate landed");
        self.nudge(entry.repo, entry.base_branch.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests;
