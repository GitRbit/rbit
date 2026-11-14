//! Compare a fork with its immediate parent's default branch and only fast-forward it.

use std::time::Duration;

use rbit_core::{OwnedRepository, PushSource, RefUpdate, User};
use rbit_git::{
    divergence::Divergence,
    merge::{MergeEnvironment, MergeError, writes::fast_forward_fork},
    overlay::Overlay,
};

use super::ForkService;
use crate::{RepoContext, ServiceError, mirrors::PUSH_REFUSED, protection::PushActor};

/// Which branches and tips a maintainer saw when asking to sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkSyncExpectation {
    pub branch: String,
    pub fork_tip: String,
    pub upstream_branch: String,
    pub upstream_tip: String,
}

#[derive(Debug)]
pub struct ForkSync {
    pub upstream: OwnedRepository,
    pub expected: ForkSyncExpectation,
    pub ahead: usize,
    pub behind: usize,
    pub refusal: Option<String>,
}

impl ForkService {
    /// The default branches' divergence, when both exist and the actor can edit the fork.
    pub async fn sync_status(&self, actor: &User, fork: &RepoContext) -> Result<Option<ForkSync>, ServiceError> {
        if !fork.can_write_content() || fork.viewer != Some(actor.id()) || self.database.mirrors().is_mirror(fork.repo.repo.id).await? {
            return Ok(None);
        }
        let Some(upstream) = self.open_upstream(actor, fork).await? else { return Ok(None) };
        let (fork_lease, upstream_lease) = self.sync_leases(fork, &upstream).await;
        let Some(expected) = default_tips(fork, &upstream).await? else { return Ok(None) };
        // Check visibility again after reading tips so commits first published after
        // upstream became private are not reported to existing forks.
        if !self.upstream_still_public(actor, fork, &upstream).await? {
            return Ok(None);
        }
        let (ahead, behind) = if expected.fork_tip == expected.upstream_tip {
            (0, 0)
        } else {
            let overlay = Overlay::create(&self.config.storage.tmp_dir(), &[&fork.disk_path, &upstream.disk_path])
                .await?
                .holding(fork_lease)
                .holding(upstream_lease);
            let Divergence { ahead, behind, .. } =
                overlay.reader().divergence(expected.upstream_tip.clone(), expected.fork_tip.clone()).await?;
            (ahead, behind)
        };
        let refusal =
            if ahead == 0 && behind > 0 { self.protection.branch_update_refusal(fork, actor, &expected.branch).await? } else { None };
        Ok(Some(ForkSync { upstream: upstream.repo, expected, ahead, behind, refusal }))
    }

    /// Update the fork's current default branch from its upstream's current default branch.
    /// The form's expected branches and tips must still match and Git checks ancestry again.
    pub async fn sync(&self, actor: &User, fork: &RepoContext, expected: &ForkSyncExpectation) -> Result<(), ServiceError> {
        if !fork.access.can_write() || fork.viewer != Some(actor.id()) {
            return Err(ServiceError::Forbidden);
        }
        fork.ensure_writable()?;
        if self.database.mirrors().is_mirror(fork.repo.repo.id).await? {
            return Err(ServiceError::Invalid(PUSH_REFUSED.to_owned()));
        }
        let upstream = self.open_upstream(actor, fork).await?.ok_or(ServiceError::NotFound)?;
        let (fork_lease, upstream_lease) = self.sync_leases(fork, &upstream).await;
        if default_tips(fork, &upstream).await?.as_ref() != Some(expected) {
            return Err(tips_changed());
        }
        // Recheck public visibility after reading the tips. The commit we will copy
        // must already have been reachable before this visibility check.
        if !self.upstream_still_public(actor, fork, &upstream).await? {
            return Err(tips_changed());
        }
        if expected.fork_tip == expected.upstream_tip {
            return Ok(());
        }
        let update = RefUpdate {
            name: format!("refs/heads/{}", expected.branch),
            before: Some(expected.fork_tip.clone()),
            after: Some(expected.upstream_tip.clone()),
        };
        let pusher = PushActor { user: Some(actor), deploy_key: false, access: fork.access };
        let verdict = self.protection.check_push(fork, &pusher, std::slice::from_ref(&update)).await?;
        if let Some(refusal) = verdict.refusals.into_iter().next() {
            return Err(ServiceError::Invalid(refusal.reason));
        }
        let env = MergeEnvironment {
            git_binary: self.config.repository.git_binary.clone(),
            tmp_dir: self.config.storage.tmp_dir(),
            timeout: Duration::from_secs(self.config.pulls.merge_timeout_seconds.max(1)),
        };
        let overlay =
            Overlay::create(&env.tmp_dir, &[&fork.disk_path, &upstream.disk_path]).await?.holding(fork_lease).holding(upstream_lease);
        fast_forward_fork(&env, &overlay, &fork.disk_path, &expected.branch, &expected.fork_tip, &expected.upstream_tip).await.map_err(
            |error| match error {
                MergeError::BaseMoved => tips_changed(),
                MergeError::Unrelated => {
                    ServiceError::Invalid("The fork has commits of its own. Compare the branches before merging.".to_owned())
                }
                other => ServiceError::internal(other),
            },
        )?;
        self.pushes.record(fork.repo.repo.id, PushSource::User(actor.id()), vec![update]).await;
        Ok(())
    }

    async fn open_upstream(&self, actor: &User, fork: &RepoContext) -> Result<Option<RepoContext>, ServiceError> {
        let Some(parent) = self.parent(Some(actor), fork).await? else { return Ok(None) };
        // A fork has independent readers, even when both repositories are private.
        if !parent.repo.visibility.is_public() {
            return Ok(None);
        }
        let upstream = match self.repositories.open(Some(actor), parent.owner.name.as_str(), parent.repo.name.as_str()).await {
            Ok(repo) if repo.repo.repo.id == parent.repo.id && repo.repo.repo.visibility.is_public() => repo,
            Ok(_) | Err(ServiceError::NotFound) => return Ok(None),
            Err(error) => return Err(error),
        };
        Ok(Some(upstream))
    }

    async fn upstream_still_public(&self, actor: &User, fork: &RepoContext, upstream: &RepoContext) -> Result<bool, ServiceError> {
        let expected = &upstream.repo.repo;
        Ok(self.parent(Some(actor), fork).await?.is_some_and(|parent| {
            parent.repo.id == expected.id && parent.repo.visibility.is_public() && parent.repo.default_branch == expected.default_branch
        }))
    }

    async fn sync_leases(
        &self,
        fork: &RepoContext,
        upstream: &RepoContext,
    ) -> (rbit_git::lease::RepositoryLease, rbit_git::lease::RepositoryLease) {
        tokio::join!(self.storage.lease(fork.repo.repo.id), self.storage.lease(upstream.repo.repo.id))
    }
}

async fn default_tips(fork: &RepoContext, upstream: &RepoContext) -> Result<Option<ForkSyncExpectation>, ServiceError> {
    let branch = fork.repo.repo.default_branch.clone();
    let upstream_branch = upstream.repo.repo.default_branch.clone();
    let (fork_tip, upstream_tip) =
        tokio::try_join!(fork.reader.branch_commit_id(branch.clone()), upstream.reader.branch_commit_id(upstream_branch.clone()))?;
    Ok(fork_tip.zip(upstream_tip).map(|(fork_tip, upstream_tip)| ForkSyncExpectation { branch, fork_tip, upstream_branch, upstream_tip }))
}

fn tips_changed() -> ServiceError {
    ServiceError::Invalid("The fork or upstream branch changed. Reload the fork and try again.".to_owned())
}
