//! The storage adapter of pull request trust (§11.1 of the Actions design): the facts
//! `rbit_actions::trust::PullRequestTrust` decides from, read now from the database.
//!
//! * The actor is whoever caused the event: the pull request's author for `opened`,
//!   `reopened`, `edited`, `ready_for_review` and `closed` — never the maintainer who
//!   clicked, so closing a fork's pull request cannot run its code with secrets — and the
//!   pusher of the new head for `synchronize` (unknown for a deploy key: untrusted).
//! * Access is read now, not when the pull request was opened: a former writer is a reader.
//! * What the decision is, and what an untrusted run is granted whoever approves it, is
//!   `rbit_actions::trust`.
//!
//! Workstream T (triggers and runs) owns this file.

use rbit_actions::trust::{ActorFacts, PullRequestTrust, RunTrust, TrustFacts};
use rbit_core::access::repo_access;
use rbit_core::{AccountId, repository::OwnedRepository};
use rbit_storage::sqlite::pulls::{PullFilter, PullStateFilter};

use super::ActionsService;
use crate::{ServiceError, repositories::viewer_grants};

impl ActionsService {
    /// How a `pull_request` run of `repo` may run: `actor` caused the event (`None` when
    /// unknown), `author` opened the pull request (`None` once their account is gone) and
    /// `is_fork` says its head is in another repository.
    pub async fn classify_pull_request(
        &self,
        repo: &OwnedRepository,
        actor: Option<AccountId>,
        author: Option<AccountId>,
        is_fork: bool,
    ) -> Result<RunTrust, ServiceError> {
        PullRequestTrust::classify(&StoredFacts { service: self, repo }, actor, author, is_fork).await
    }
}

/// `repo`'s view of accounts, from the database.
struct StoredFacts<'a> {
    service: &'a ActionsService,
    repo: &'a OwnedRepository,
}

impl TrustFacts for StoredFacts<'_> {
    type Account = AccountId;
    type Error = ServiceError;

    async fn actor(&self, id: AccountId) -> Result<Option<ActorFacts>, ServiceError> {
        let database = &self.service.ctx().database;
        let Some(user) = database.accounts().find_user(id).await? else { return Ok(None) };
        let suspended = database.accounts().is_suspended(id).await?;
        let grants = viewer_grants(&database.repositories(), Some(&user), self.repo).await?;
        Ok(Some(ActorFacts { access: repo_access(Some(&user), self.repo, &grants), suspended }))
    }

    async fn is_suspended(&self, id: AccountId) -> Result<bool, ServiceError> {
        Ok(self.service.ctx().database.accounts().is_suspended(id).await?)
    }

    async fn is_trusted_user(&self, id: AccountId) -> Result<bool, ServiceError> {
        Ok(self.service.ctx().database.actions_settings().is_trusted(self.repo.repo.id, id).await?)
    }

    // Actions: settings API. Under `all_external_contributors` only the trusted list vouches.
    async fn merged_pull_vouches(&self) -> Result<bool, ServiceError> {
        Ok(self.service.effective_policies(self.repo.repo.id).await?.approval_policy.merged_pull_vouches())
    }

    async fn has_merged_pull(&self, id: AccountId) -> Result<bool, ServiceError> {
        let merged = PullFilter { state: PullStateFilter::Merged, author: Some(id), ..PullFilter::default() };
        Ok(self.service.ctx().database.pulls().count(self.repo.repo.id, &merged).await? > 0)
    }
}
