//! `pr ensure` (agent plan step 4.3): publish or recover one pull request for a pair of
//! branches, safely retried.
//!
//! * An open pull request from the same head (repository and branch) into the same base
//!   branch is returned, not refused.
//! * With a request id, the answer is remembered for a week per account: replaying the id
//!   with the same inputs returns the same pull request, even after it was created by the
//!   first call whose response was lost; replaying it with other inputs is a `conflict`.

use std::fmt::Write as _;

use rbit_core::{Issue, IssueNumber, Timestamp, User, pull::PullRequest};
use sha2::{Digest as _, Sha256};

use super::{NewPullInput, PullService};
use crate::{RepoContext, ServiceError};

/// How `ensure` found its pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ensured {
    Created(IssueNumber),
    /// An open pull request already proposed these branches.
    Existing(IssueNumber),
    /// The request id was seen before with these inputs.
    Replayed(IssueNumber),
}

impl Ensured {
    #[must_use]
    pub fn number(self) -> IssueNumber {
        match self {
            Self::Created(number) | Self::Existing(number) | Self::Replayed(number) => number,
        }
    }
}

/// The fingerprint of a request's inputs: the repositories, branches and text.
fn fingerprint(base: &RepoContext, head: &RepoContext, input: &NewPullInput) -> String {
    let mut hash = Sha256::new();
    for part in [
        base.repo.repo.id.0.to_string(),
        head.repo.repo.id.0.to_string(),
        input.base_branch.clone(),
        input.head_branch.clone(),
        input.title.clone(),
        input.body.clone(),
        input.draft.to_string(),
    ] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    hash.finalize().iter().take(16).fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

impl PullService {
    /// The open pull request from `head`'s `input.head_branch` into `base`'s
    /// `input.base_branch`: an existing one, or a new one from `input`. `request_id` makes
    /// a retry return the first answer (see the module documentation).
    pub async fn ensure(
        &self,
        actor: &User,
        base: &RepoContext,
        head: &RepoContext,
        input: NewPullInput,
        request_id: Option<&str>,
    ) -> Result<Ensured, ServiceError> {
        let requests = self.ctx().database.actions_runs();
        let print = fingerprint(base, head, &input);
        if let Some(id) = request_id
            && let Some(seen) = requests.find_request(actor.id(), id, Timestamp::now()).await?
        {
            if seen.fingerprint != print {
                return Err(ServiceError::Conflict("this request id was used for another request"));
            }
            if let Ok(number) = IssueNumber::parse(&seen.answer) {
                return Ok(Ensured::Replayed(number));
            }
        }
        let open = self.ctx().database.pulls().open_with_head(head.repo.repo.id, &input.head_branch).await?;
        let existing = open
            .iter()
            .find(|(issue, pull)| issue.repo_id == base.repo.repo.id && pull.base_branch == input.base_branch)
            .map(|(issue, _)| issue.number);
        let ensured = match existing {
            Some(number) => Ensured::Existing(number),
            None => Ensured::Created(self.create(actor, base, head, input).await?.0.number),
        };
        if let Some(id) = request_id {
            requests.remember_request(actor.id(), id, &print, &ensured.number().get().to_string(), Timestamp::now()).await?;
        }
        Ok(ensured)
    }
}

impl PullService {
    /// The pull requests of `repo` a commit belongs to: as head, merge commit or merged
    /// head, open or not, newest first (agent plan step 4.5). `repo` was opened for the
    /// caller, so only a reader gets here.
    pub async fn with_commit(&self, repo: &RepoContext, sha: &str) -> Result<Vec<(Issue, PullRequest)>, ServiceError> {
        Ok(self.ctx().database.pulls().with_commit(repo.repo.repo.id, sha).await?)
    }
}
