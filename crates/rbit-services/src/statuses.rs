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
//! * Reading resolves a branch, tag or commit id ([`StatusService::resolve`]); the
//!   combined state is [`rbit_core::status::combined_state`] over the latest status of
//!   each context.
//!
//! Requests are already throttled per account by the API ([`crate::ThrottleScope::ApiRequest`]);
//! the per-context cap bounds what one commit can accumulate.

pub mod hooks;

use std::collections::HashMap;

use rbit_core::{
    Account, RepoId, User,
    status::{CommitStatus, MAX_STATUSES_PER_CONTEXT, StatusReport, StatusState, combined_state, parse_sha},
};
use rbit_git::GitError;

use crate::{DomainEvent, RepoContext, ServiceContext, ServiceError};

/// A status as submitted (the REST API's body).
#[derive(Debug, Clone, Default)]
pub struct NewStatus {
    pub state: String,
    pub target_url: Option<String>,
    pub description: Option<String>,
    /// Absent or blank: `default`.
    pub context: Option<String>,
}

/// A commit's combined status: the overall state and the latest status of each context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombinedStatus {
    pub sha: String,
    pub state: StatusState,
    /// The latest status of each context, by context.
    pub statuses: Vec<CommitStatus>,
}

impl CombinedStatus {
    fn new(sha: String, statuses: Vec<CommitStatus>) -> Self {
        Self { state: combined_state(&statuses), sha, statuses }
    }
}

/// One page of a commit's statuses, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusPage {
    pub sha: String,
    pub statuses: Vec<CommitStatus>,
    /// Statuses across all pages.
    pub total: u64,
}

#[derive(Debug, Clone)]
pub struct StatusService {
    ctx: ServiceContext,
}

impl StatusService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }

    /// Records a status of commit `sha` (a full id) for `actor`, who needs write access.
    pub async fn create(&self, actor: &User, repo: &RepoContext, sha: &str, input: &NewStatus) -> Result<CommitStatus, ServiceError> {
        if !repo.access.can_write() {
            return Err(ServiceError::Forbidden);
        }
        let invalid = |error: rbit_core::status::StatusError| ServiceError::Invalid(error.to_string());
        let sha = parse_sha(sha).map_err(invalid)?;
        let report = StatusReport::parse(&input.state, input.context.as_deref(), input.description.as_deref(), input.target_url.as_deref())
            .map_err(invalid)?;
        let no_commit = || ServiceError::Invalid(format!("no commit found for SHA {sha}"));
        match repo.reader.commit_id(sha.clone()).await {
            Ok(found) if found == sha => {}
            Ok(_) | Err(GitError::NotFound(_)) => return Err(no_commit()),
            Err(error) => return Err(error.into()),
        }
        let store = self.ctx.database.commit_statuses();
        let Some(status) = store.create(repo.repo.repo.id, &sha, &report, actor.id(), MAX_STATUSES_PER_CONTEXT).await? else {
            return Err(ServiceError::Invalid(format!(
                "this SHA and context has reached the maximum number of statuses ({MAX_STATUSES_PER_CONTEXT})"
            )));
        };
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

    // Actions: triggers (T).
    /// Records `report` on commit `sha` (a full id of `repo`, not checked) as rbit Actions:
    /// no creator and no permission check — the caller is the Actions service reporting its
    /// own jobs. `None` when the commit already has the most statuses one context may have.
    #[expect(dead_code, reason = "the Actions job statuses are its only caller, and they come next")]
    pub(crate) async fn create_as_system(
        &self,
        repo: RepoId,
        sha: &str,
        report: &StatusReport,
    ) -> Result<Option<CommitStatus>, ServiceError> {
        let store = self.ctx.database.commit_statuses();
        let Some(status) = store.create_as_system(repo, sha, report, MAX_STATUSES_PER_CONTEXT).await? else {
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
        let store = self.ctx.database.commit_statuses();
        let (statuses, total) = tokio::try_join!(store.list(repo.repo.repo.id, &sha, offset, limit), store.count(repo.repo.repo.id, &sha))?;
        Ok(StatusPage { sha, statuses, total })
    }

    /// The combined status of the commit `reference` names.
    pub async fn combined(&self, repo: &RepoContext, reference: &str) -> Result<CombinedStatus, ServiceError> {
        let sha = self.resolve(repo, reference).await?;
        self.combined_for(repo, &sha).await
    }

    /// The combined status of commit `sha` (a full id; not resolved).
    pub async fn combined_for(&self, repo: &RepoContext, sha: &str) -> Result<CombinedStatus, ServiceError> {
        let statuses = self.ctx.database.commit_statuses().latest(repo.repo.repo.id, sha).await?;
        Ok(CombinedStatus::new(sha.to_owned(), statuses))
    }

    /// The combined status of every commit of `shas` that has statuses (one query), for
    /// commit lists. Commits without statuses are absent.
    pub async fn combined_of(&self, repo: &RepoContext, shas: &[String]) -> Result<HashMap<String, CombinedStatus>, ServiceError> {
        let mut by_sha: HashMap<String, Vec<CommitStatus>> = HashMap::new();
        for status in self.ctx.database.commit_statuses().latest_of(repo.repo.repo.id, shas).await? {
            by_sha.entry(status.sha.clone()).or_default().push(status);
        }
        Ok(by_sha.into_iter().map(|(sha, statuses)| (sha.clone(), CombinedStatus::new(sha, statuses))).collect())
    }

    /// The accounts that created `statuses` (deleted ones are absent).
    pub async fn creators(&self, statuses: &[CommitStatus]) -> Result<Vec<Account>, ServiceError> {
        let mut ids: Vec<_> = statuses.iter().filter_map(|status| status.creator).collect();
        ids.sort_unstable();
        ids.dedup();
        Ok(self.ctx.database.accounts().find_accounts(&ids).await?)
    }
}
