//! Commit statuses: external CI reports the state of a check on a commit, and pages, the
//! merge box and the API read them back.
//!
//! Milestone 3 workstream S (commit statuses and required checks) owns this file and
//! `statuses/**`.
//!
//! Rules:
//! * Creating a status needs write access to the repository (deploy keys cannot post;
//!   the REST API also needs a token with the `write` scope). Everyone who can read the
//!   repository can read its statuses; hidden repositories are `NotFound` before this
//!   service is reached.
//! * A status is posted on a full commit id that exists in the repository (for a pull
//!   request from a fork, the head is in the base repository as `refs/pull/{n}/head`).
//!   Values are validated by [`StatusReport::parse`]; at most [`MAX_STATUSES_PER_CONTEXT`]
//!   reports are kept per commit and context, further ones are refused, as on GitHub.
//! * Each new status publishes [`DomainEvent::CommitStatus`]: the `status` webhook
//!   ([`hooks::payload`]) and the pull request service (which re-evaluates the pull
//!   requests whose head it is) react to it.
//! * rbit Actions posts the statuses of its jobs through the crate-private
//!   [`StatusService::create_as_system`]: no creator, `source = actions`, and a webhook
//!   `sender` of `rbit-actions[bot]`. A status must not look like something a person typed,
//!   and the people behind a run (a fork's author, nobody for a schedule) may not be able to
//!   write to the repository. Required checks treat these statuses like any other.
//!   A workflow task posting through the API with its token (`contents: write`) is recorded
//!   the same way, by [`StatusService::create_for_task`], after the checks people pass.
//! * Reading resolves a branch, tag or commit id ([`StatusService::resolve`]); the
//!   combined state is [`rbit_core::status::combined_state`] over the latest status of
//!   each context.
//!
//! These rules are decided by [`rbit_collaboration::statuses::StatusApplication`] over the
//! `CommitStatusRepository` port, which the storage crate implements. This service is the
//! composition adapter: it resolves references, parses reports, maps errors, logs and
//! publishes [`DomainEvent::CommitStatus`].
//!
//! Requests are already throttled per account by the API ([`crate::ThrottleScope::ApiRequest`]);
//! the per-context cap bounds what one commit can accumulate.

pub mod hooks;

use std::collections::HashMap;

use rbit_collaboration::statuses::{StatusAccess, StatusApplication, StatusApplicationError, creator_ids};
use rbit_core::{
    Account, RepoId, User,
    status::{CommitStatus, StatusError, StatusReport},
};
use rbit_git::GitError;
use rbit_storage::{StorageError, sqlite::statuses::CommitStatusStore};

pub use rbit_collaboration::statuses::{CombinedStatus, StatusPage};

use crate::{DomainEvent, RepoContext, ServiceContext, ServiceError, actions::tokens::TaskGrant};

/// A status as submitted (the REST API's body).
#[derive(Debug, Clone, Default)]
pub struct NewStatus {
    pub state: String,
    pub target_url: Option<String>,
    pub description: Option<String>,
    /// Absent or blank: `default`.
    pub context: Option<String>,
}

impl NewStatus {
    fn report(&self) -> Result<StatusReport, StatusError> {
        StatusReport::parse(&self.state, self.context.as_deref(), self.description.as_deref(), self.target_url.as_deref())
    }
}

fn error(error: StatusApplicationError<StorageError, GitError>) -> ServiceError {
    match error {
        StatusApplicationError::Store(error) => error.into(),
        StatusApplicationError::Commit(error) => error.into(),
        StatusApplicationError::Invalid(message) => ServiceError::Invalid(message),
        StatusApplicationError::Forbidden => ServiceError::Forbidden,
        StatusApplicationError::Archived => ServiceError::Archived,
    }
}

/// The full id of the commit `sha` names in `repo`; `None` when there is none.
async fn commit_of(repo: &RepoContext, sha: String) -> Result<Option<String>, GitError> {
    match repo.reader.commit_id(sha).await {
        Ok(found) => Ok(Some(found)),
        Err(GitError::NotFound(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

#[derive(Debug, Clone)]
pub struct StatusService {
    ctx: ServiceContext,
}

impl StatusService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }

    fn application(&self, access: StatusAccess) -> StatusApplication<CommitStatusStore> {
        StatusApplication::new(self.ctx.database.commit_statuses(), access)
    }

    /// Writes by the request's caller, with its access to `repo`.
    fn for_caller(&self, repo: &RepoContext) -> StatusApplication<CommitStatusStore> {
        let id = repo.repo.repo.id;
        self.application(StatusAccess { repo: id, can_write: repo.access.can_write(), archived: repo.repo.repo.is_archived() })
    }

    /// Reads and system writes, which check no permission.
    fn system(&self, repo: RepoId) -> StatusApplication<CommitStatusStore> {
        self.application(StatusAccess { repo, can_write: false, archived: false })
    }

    /// Records a status of commit `sha` (a full id) for `actor`, who needs write access.
    pub async fn create(&self, actor: &User, repo: &RepoContext, sha: &str, input: &NewStatus) -> Result<CommitStatus, ServiceError> {
        let application = self.for_caller(repo);
        let status = application.create(actor.id(), sha, || input.report(), |sha| commit_of(repo, sha)).await.map_err(error)?;
        tracing::info!(
            repo = %repo.repo.full_name(),
            sha = %status.sha,
            context = %status.context,
            state = %status.state,
            by = %actor.name(),
            "commit status created"
        );
        self.ctx.events.publish(DomainEvent::CommitStatus {
            repo: repo.repo.repo.id,
            status: status.id,
            sha: status.sha.clone(),
            actor: Some(actor.id()),
        });
        Ok(status)
    }

    // Actions: task tokens.
    /// Records a status a workflow task posted through the API, with the checks of
    /// [`create`](Self::create), as rbit Actions: no creator and `source = actions`, like the
    /// statuses of its jobs. The run's people did not type it, and may not be able to write.
    /// `repo` is the task's own repository, opened for it with its grant.
    pub async fn create_for_task(
        &self,
        grant: &TaskGrant,
        repo: &RepoContext,
        sha: &str,
        input: &NewStatus,
    ) -> Result<CommitStatus, ServiceError> {
        let application = self.for_caller(repo);
        let status = application.create_for_task(grant.repo, sha, || input.report(), |sha| commit_of(repo, sha)).await.map_err(error)?;
        tracing::info!(
            repo = %repo.repo.full_name(),
            sha = %status.sha,
            context = %status.context,
            state = %status.state,
            task = grant.task.0,
            "commit status created by a workflow task"
        );
        self.ctx.events.publish(DomainEvent::CommitStatus { repo: grant.repo, status: status.id, sha: status.sha.clone(), actor: None });
        Ok(status)
    }

    // Actions: triggers (T).
    /// Records `report` on commit `sha` (a full id of `repo`, not checked) as rbit Actions:
    /// no creator and no permission check — the caller is the Actions service reporting its
    /// own jobs. `None` when the commit already has the most statuses one context may have.
    pub(crate) async fn create_as_system(
        &self,
        repo: RepoId,
        sha: &str,
        report: &StatusReport,
    ) -> Result<Option<CommitStatus>, ServiceError> {
        let Some(status) = self.system(repo).record_system(sha, report).await? else {
            tracing::warn!(repo = repo.0, sha, context = %report.context.as_str(), "commit status cap reached; Actions status dropped");
            return Ok(None);
        };
        tracing::debug!(repo = repo.0, sha = %status.sha, context = %status.context, state = %status.state, "Actions commit status created");
        self.ctx.events.publish(DomainEvent::CommitStatus { repo, status: status.id, sha: status.sha.clone(), actor: None });
        Ok(Some(status))
    }

    /// The full id of the commit `reference` (a branch, tag or commit id) names;
    /// [`ServiceError::NotFound`] if there is none.
    pub async fn resolve(&self, repo: &RepoContext, reference: &str) -> Result<String, ServiceError> {
        Ok(repo.reader.commit_id(reference.to_owned()).await?)
    }

    /// The statuses of the commit `reference` names, newest first.
    pub async fn list(&self, repo: &RepoContext, reference: &str, offset: u64, limit: u32) -> Result<StatusPage, ServiceError> {
        let sha = self.resolve(repo, reference).await?;
        Ok(self.system(repo.repo.repo.id).page(sha, offset, limit).await?)
    }

    /// The combined status of the commit `reference` names.
    pub async fn combined(&self, repo: &RepoContext, reference: &str) -> Result<CombinedStatus, ServiceError> {
        let sha = self.resolve(repo, reference).await?;
        self.combined_for(repo, &sha).await
    }

    /// The combined status of commit `sha` (a full id; not resolved).
    pub async fn combined_for(&self, repo: &RepoContext, sha: &str) -> Result<CombinedStatus, ServiceError> {
        Ok(self.system(repo.repo.repo.id).combined(sha.to_owned()).await?)
    }

    /// The combined status of every commit of `shas` that has statuses (one query), for
    /// commit lists. Commits without statuses are absent.
    pub async fn combined_of(&self, repo: &RepoContext, shas: &[String]) -> Result<HashMap<String, CombinedStatus>, ServiceError> {
        Ok(self.system(repo.repo.repo.id).combined_of(shas).await?)
    }

    /// Report `id` of `repo`, for the `status` webhook (no permission check).
    pub(crate) async fn find(&self, repo: RepoId, id: rbit_core::status::StatusId) -> Result<Option<CommitStatus>, ServiceError> {
        Ok(self.system(repo).find(id).await?)
    }

    /// The accounts that created `statuses` (deleted ones are absent).
    pub async fn creators(&self, statuses: &[CommitStatus]) -> Result<Vec<Account>, ServiceError> {
        Ok(self.ctx.database.accounts().find_accounts(&creator_ids(statuses)).await?)
    }
}
