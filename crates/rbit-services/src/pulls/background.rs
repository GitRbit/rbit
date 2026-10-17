//! The pull request service's background work: reacting to pushes (head moved, head
//! deleted, base moved, merged outside rbit), the mergeability worker and the start-up
//! sweep of temporary directories.
//!
//! Milestone 2 workstream P (pull requests) owns this file.
//!
//! Every handler reads current state (events carry ids only) and is idempotent, so a
//! push reported twice (two racing pushes see each other's updates) does no harm.

use std::{sync::Arc, sync::atomic::Ordering, time::Duration};

use rbit_core::{
    AccountId, Issue, RefUpdate, RepoId, StateReason, TimelineEvent, Timestamp,
    issue::IssueAction,
    pull::{Mergeability, PullRequest},
};
use rbit_git::{
    merge::{MergeEnvironment, MergeError, TrialMerge, trial},
    overlay,
};
use rbit_storage::sqlite::pulls::MergeRecord;
use tokio::sync::broadcast::error::RecvError;

use super::PullService;
use crate::{DomainEvent, RepoContext, ServiceError};

/// How often the mergeability worker looks for `unknown` rows without being woken.
pub const MERGEABILITY_TICK: Duration = Duration::from_secs(60);
/// Pull requests the worker picks up per pass.
const BATCH: u32 = 16;
/// How long [`PullService::shutdown`] waits for background work.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

impl PullService {
    /// Starts this service's background work (head and base updates on
    /// `RepositoryPushed`, the mergeability worker, the temporary-directory sweep). Called
    /// once from [`crate::Services::spawn_background`].
    pub fn spawn_background(&self) {
        if self.inner.started.swap(true, Ordering::SeqCst) {
            return;
        }
        // Subscribe now, so no push published after start-up is missed.
        let events = self.ctx().events.subscribe();
        let subscriber = self.clone();
        self.inner.tasks.spawn(async move { subscriber.follow_pushes(events).await });
        let worker = self.clone();
        self.inner.tasks.spawn(async move {
            if let Err(error) = overlay::sweep(&worker.ctx().config.storage.tmp_dir()).await {
                tracing::warn!(%error, "cannot sweep temporary merge directories");
            }
            worker.mergeability_worker().await;
        });
    }

    /// Stops background work (running merges and trial merges) and waits (bounded) for it
    /// to finish. Called from [`crate::Services::shutdown`] before the stores close.
    pub async fn shutdown(&self) {
        self.inner.shutdown.cancel();
        self.inner.tasks.close();
        if tokio::time::timeout(SHUTDOWN_GRACE, self.inner.tasks.wait()).await.is_err() {
            tracing::warn!("pull request background work did not stop within {SHUTDOWN_GRACE:?}");
        }
    }

    async fn follow_pushes(self, mut events: tokio::sync::broadcast::Receiver<DomainEvent>) {
        loop {
            let event = tokio::select! {
                () = self.inner.shutdown.cancelled() => return,
                event = events.recv() => event,
            };
            match event {
                Ok(DomainEvent::RepositoryPushed { repo, pusher, updates }) => {
                    if let Err(error) = self.handle_push(repo, pusher, &updates).await {
                        tracing::warn!(%error, repo = repo.0, "cannot update pull requests after a push");
                    }
                }
                Ok(_) => {}
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "pull requests missed push events; mergeability is rechecked periodically");
                    self.inner.wake.notify_one();
                }
                Err(RecvError::Closed) => return,
            }
        }
    }

    /// Applies one push to the pull requests whose head or base it moved. Public so tests
    /// can drive it without the background task.
    pub async fn handle_push(&self, repo: RepoId, pusher: Option<AccountId>, updates: &[RefUpdate]) -> Result<(), ServiceError> {
        let store = self.ctx().database.pulls();
        for update in updates {
            let Some(branch) = update.branch() else { continue };
            for (issue, pull) in store.open_with_head(repo, branch).await? {
                self.head_moved(&issue, &pull, update, pusher).await?;
            }
            for (issue, pull) in store.open_with_base(repo, branch).await? {
                self.base_moved(&issue, &pull, update, pusher).await?;
            }
        }
        self.inner.wake.notify_one();
        Ok(())
    }

    async fn head_moved(
        &self,
        issue: &Issue,
        pull: &PullRequest,
        update: &RefUpdate,
        pusher: Option<AccountId>,
    ) -> Result<(), ServiceError> {
        let Some(after) = &update.after else {
            self.inner.timeline.record(issue.id, pusher, TimelineEvent::HeadRefDeleted { branch: pull.head_branch.clone() }).await?;
            return Ok(());
        };
        let Some(base) = RepoContext::load_for_system(self.ctx(), pull.repo_id).await? else { return Ok(()) };
        let head = match pull.head_repo_id {
            Some(id) if id == pull.repo_id => base.clone(),
            Some(id) => match RepoContext::load_for_system(self.ctx(), id).await? {
                Some(head) => head,
                None => return Ok(()),
            },
            None => return Ok(()),
        };
        let mut moved = pull.clone();
        moved.head_sha.clone_from(after);
        let stored = self.store_head(&base, &head, &moved, issue.number).await?;
        if stored == pull.head_sha {
            return Ok(());
        }
        let before = pull.head_sha.clone();
        let reader = &base.reader;
        let forced = !reader.is_ancestor(before.clone(), stored.clone()).await.unwrap_or(false);
        let count = reader
            .new_commits(stored.clone(), vec![before.clone()], 10_000)
            .await
            .map_or(0, |commits| u32::try_from(commits.len()).unwrap_or(u32::MAX));
        self.ctx().database.pulls().set_head(issue.id, &stored, Timestamp::now()).await?;
        self.inner
            .timeline
            .record(issue.id, pusher, TimelineEvent::HeadPushed { before: before.clone(), after: stored.clone(), count, forced })
            .await?;
        self.publish(issue, pusher, IssueAction::Synchronized { before, after: stored.clone() });
        self.mark_outdated(issue, &base, &stored).await?;
        Ok(())
    }

    /// Review comments whose file differs at the new head become outdated.
    async fn mark_outdated(&self, issue: &Issue, base: &RepoContext, head: &str) -> Result<(), ServiceError> {
        let store = self.ctx().database.reviews();
        let comments: Vec<_> = store.comments(issue.id).await?.into_iter().filter(|comment| !comment.outdated).collect();
        if comments.is_empty() {
            return Ok(());
        }
        let paths: Vec<String> = comments.iter().map(|comment| comment.path.clone()).collect();
        let now = base.reader.path_ids(head.to_owned(), paths.clone()).await?;
        let mut outdated = Vec::new();
        for (index, comment) in comments.iter().enumerate() {
            let then = base
                .reader
                .path_ids(comment.commit_sha.clone(), vec![paths[index].clone()])
                .await
                .ok()
                .and_then(|mut ids| ids.pop().flatten());
            if then != now[index] {
                outdated.push(comment.id);
            }
        }
        store.mark_outdated(&outdated).await?;
        Ok(())
    }

    async fn base_moved(
        &self,
        issue: &Issue,
        pull: &PullRequest,
        update: &RefUpdate,
        pusher: Option<AccountId>,
    ) -> Result<(), ServiceError> {
        let store = self.ctx().database.pulls();
        store.mark_unknown(issue.id).await?;
        let Some(after) = &update.after else { return Ok(()) };
        let Some(base) = RepoContext::load_for_system(self.ctx(), pull.repo_id).await? else { return Ok(()) };
        if !base.reader.is_ancestor(pull.head_sha.clone(), after.clone()).await.unwrap_or(false) {
            return Ok(());
        }
        // The head is part of the base now: merged outside rbit.
        let record =
            MergeRecord { merged_by: pusher, strategy: None, commit: after, base: after, head: &pull.head_sha, at: Timestamp::now() };
        if store.record_merge(issue.id, &record).await? {
            let event = TimelineEvent::Merged { commit: after.clone(), strategy: None };
            self.inner.timeline.close_as_system(issue.id, pusher, StateReason::Merged, event).await?;
            tracing::info!(pull = issue.id.0, "pull request merged outside rbit");
        }
        Ok(())
    }

    // ------------------------------------------------------------- mergeability

    async fn mergeability_worker(self) {
        loop {
            if let Err(error) = self.check_unknown().await {
                tracing::warn!(%error, "cannot compute mergeability");
            }
            tokio::select! {
                () = self.inner.shutdown.cancelled() => return,
                () = self.inner.wake.notified() => {}
                () = tokio::time::sleep(MERGEABILITY_TICK) => {}
            }
        }
    }

    /// Computes the mergeability of every open, non-draft pull request whose state is
    /// unknown, at most `mergeability_concurrency` at a time. Public for tests.
    pub async fn check_unknown(&self) -> Result<usize, ServiceError> {
        let mut done = 0;
        loop {
            let pending = self.ctx().database.pulls().unknown_mergeability(BATCH).await?;
            if pending.is_empty() {
                return Ok(done);
            }
            let mut tasks = tokio::task::JoinSet::new();
            for pull in pending {
                let service = self.clone();
                let slots = Arc::clone(&self.inner.trial_slots);
                tasks.spawn(async move {
                    let Ok(_permit) = slots.acquire_owned().await else { return };
                    if let Err(error) = service.check_one(&pull).await {
                        tracing::warn!(%error, pull = pull.issue_id.0, "trial merge failed");
                        let base = pull.mergeable_base_sha.clone().unwrap_or_default();
                        let _ = service
                            .ctx()
                            .database
                            .pulls()
                            .set_mergeability(pull.issue_id, Mergeability::Error, &base, &pull.head_sha, &[])
                            .await;
                    }
                });
            }
            while tasks.join_next().await.is_some() {
                done += 1;
            }
            if self.inner.shutdown.is_cancelled() {
                return Ok(done);
            }
        }
    }

    /// Runs the trial merge of one pull request and stores the result.
    pub(super) async fn check_one(&self, pull: &PullRequest) -> Result<Mergeability, ServiceError> {
        let Some(base) = RepoContext::load_for_system(self.ctx(), pull.repo_id).await? else { return Ok(Mergeability::Unknown) };
        let store = self.ctx().database.pulls();
        let Ok(base_tip) = base.reader.commit_id(format!("refs/heads/{}", pull.base_branch)).await else {
            store.set_mergeability(pull.issue_id, Mergeability::Error, "", &pull.head_sha, &[]).await?;
            return Ok(Mergeability::Error);
        };
        let (state, paths) = self.trial_merge(&base, &base_tip, &pull.head_sha).await?;
        store.set_mergeability(pull.issue_id, state, &base_tip, &pull.head_sha, &paths).await?;
        Ok(state)
    }

    /// The trial merge of `head` into `base_tip` in `base`.
    pub(super) async fn trial_merge(
        &self,
        base: &RepoContext,
        base_tip: &str,
        head: &str,
    ) -> Result<(Mergeability, Vec<String>), ServiceError> {
        let _lease = self.ctx().repositories.lease(base.repo.repo.id).await;
        match trial(&self.merge_environment(), &base.disk_path, base_tip, head).await {
            Ok(TrialMerge::Clean) => Ok((Mergeability::Clean, Vec::new())),
            Ok(TrialMerge::Conflict(paths)) => Ok((Mergeability::Conflict, paths)),
            Err(MergeError::Unrelated) => Ok((Mergeability::Error, Vec::new())),
            Err(error) => Err(ServiceError::internal(error)),
        }
    }

    pub(super) fn merge_environment(&self) -> MergeEnvironment {
        let config = &self.ctx().config;
        MergeEnvironment {
            git_binary: config.repository.git_binary.clone(),
            tmp_dir: config.storage.tmp_dir(),
            timeout: Duration::from_secs(config.pulls.merge_timeout_seconds.max(1)),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_worker_wakes_up_regularly() {
        assert!(super::MERGEABILITY_TICK.as_secs() >= 10);
    }
}
