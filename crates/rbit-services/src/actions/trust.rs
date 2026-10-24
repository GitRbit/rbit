//! Whether a pull request run is trusted (§11.1 of the Actions design), from the actor's
//! current access, the repository's trusted users and earlier merged pull requests; the pure
//! decision is `rbit_core::actions::classify_pull_request`.
//!
//! * The actor is whoever caused the event: the pull request's author for `opened`,
//!   `reopened`, `edited`, `ready_for_review` and `closed` — never the maintainer who
//!   clicked, so closing a fork's pull request cannot run its code with secrets — and the
//!   pusher of the new head for `synchronize` (unknown for a deploy key: untrusted).
//! * Access is read now, not when the pull request was opened: a former writer is a reader.
//! * An untrusted run gets a read-only token, no secrets and no variables
//!   (`Run::trusted == false`), whoever approves it. It waits for approval unless its author
//!   is vouched for — a trusted user of the repository, or the author of a merged pull
//!   request into it — and pushed the head themselves. Suspended authors always wait.
//!
//! Workstream T (triggers and runs) owns this file.

use rbit_auth::repo_access;
use rbit_core::{
    AccountId,
    actions::{AuthorStanding, RunTrust, TriggeringActor, classify_pull_request},
    repository::OwnedRepository,
};
use rbit_storage::sqlite::pulls::{PullFilter, PullStateFilter};

use super::ActionsService;
use crate::{ServiceError, repositories::viewer_grants};

impl ActionsService {
    /// How a `pull_request` run of `repo` may run: `actor` caused the event (`None` when
    /// unknown) and `author` opened the pull request (`None` once their account is gone).
    pub async fn classify_pull_request(
        &self,
        repo: &OwnedRepository,
        actor: Option<AccountId>,
        author: Option<AccountId>,
    ) -> Result<RunTrust, ServiceError> {
        let actor = match actor {
            Some(id) => self.triggering_actor(repo, id, author).await?,
            None => TriggeringActor::Unknown,
        };
        let standing = match author {
            Some(id) => self.author_standing(repo, id).await?,
            None => AuthorStanding::Unknown,
        };
        Ok(classify_pull_request(actor, standing))
    }

    async fn triggering_actor(
        &self,
        repo: &OwnedRepository,
        id: AccountId,
        author: Option<AccountId>,
    ) -> Result<TriggeringActor, ServiceError> {
        let database = &self.ctx().database;
        let Some(user) = database.accounts().find_user(id).await? else { return Ok(TriggeringActor::Unknown) };
        let suspended = database.accounts().is_suspended(id).await?;
        let grants = viewer_grants(&database.repositories(), Some(&user), repo).await?;
        let access = repo_access(Some(&user), repo, &grants);
        Ok(TriggeringActor::Known { access, suspended, is_author: author == Some(id) })
    }

    async fn author_standing(&self, repo: &OwnedRepository, author: AccountId) -> Result<AuthorStanding, ServiceError> {
        let database = &self.ctx().database;
        if database.accounts().is_suspended(author).await? {
            return Ok(AuthorStanding::Suspended);
        }
        if database.actions_settings().is_trusted(repo.repo.id, author).await? {
            return Ok(AuthorStanding::Vouched);
        }
        let merged = PullFilter { state: PullStateFilter::Merged, author: Some(author), ..PullFilter::default() };
        if database.pulls().count(repo.repo.id, &merged).await? > 0 {
            return Ok(AuthorStanding::Vouched);
        }
        Ok(AuthorStanding::Unknown)
    }
}
