//! Who and what a thread may name: users by name, a repository's labels by id, people
//! who may be assigned, and the thread a comment belongs to. Persistence and the
//! repository access of other people stay behind [`ParticipantPort`].

use std::future::Future;

use rbit_contracts::{AccountId, RepoId};
use rbit_identity_models::{Account, User};

use crate::{comments::CommentId, labels::Label, labels::LabelId, threads::IssueNumber};

/// Most people [`IssueParticipants::assignable`] lists.
pub const MAX_ASSIGNABLE: u32 = 100;

/// Account lookup by name, independent of any repository.
pub trait AccountNamePort: Sync {
    type Error;

    /// Accounts (people and organisations) among `names`, case-insensitively.
    fn accounts_named(&self, names: &[String]) -> impl Future<Output = Result<Vec<Account>, Self::Error>> + Send;
}

/// The people among `names` (case-insensitively); a name that is unknown or an
/// organisation is invalid.
pub async fn users_named<P: AccountNamePort>(port: &P, names: &[String]) -> Result<Vec<Account>, ParticipantError<P::Error>> {
    let accounts = port.accounts_named(names).await.map_err(ParticipantError::Store)?;
    if let Some(missing) =
        names.iter().find(|name| !accounts.iter().any(|a| !a.is_organization() && a.name.as_str().eq_ignore_ascii_case(name)))
    {
        return Err(ParticipantError::Invalid(format!("there is no user called “{missing}”")));
    }
    Ok(accounts.into_iter().filter(|a| !a.is_organization()).collect())
}

/// Lookups for one repository. `can_push` decides with the person's current access.
pub trait ParticipantPort: Sync {
    type Error;

    fn find_label(&self, repo: RepoId, id: LabelId) -> impl Future<Output = Result<Option<Label>, Self::Error>> + Send;
    fn find_active_user(&self, id: AccountId) -> impl Future<Output = Result<Option<User>, Self::Error>> + Send;
    fn can_push(&self, user: &User) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// People with write access whose names start with `prefix`, by name, at most `limit`.
    fn assignable(&self, repo: RepoId, prefix: &str, limit: u32) -> impl Future<Output = Result<Vec<Account>, Self::Error>> + Send;
    fn comment_thread(&self, repo: RepoId, id: CommentId) -> impl Future<Output = Result<Option<IssueNumber>, Self::Error>> + Send;
}

/// Refusals keep the legacy service's text; storage errors pass through.
#[derive(Debug)]
pub enum ParticipantError<E> {
    Store(E),
    Invalid(String),
    NotFound,
}

/// Participant decisions over a port for one repository.
#[derive(Debug)]
pub struct IssueParticipants<'a, P> {
    port: &'a P,
    repo: RepoId,
}

impl<'a, P: ParticipantPort> IssueParticipants<'a, P> {
    pub fn new(port: &'a P, repo: RepoId) -> Self {
        Self { port, repo }
    }

    /// The labels of this repository with these ids, in order; any other id is invalid.
    pub async fn labels(&self, ids: &[LabelId]) -> Result<Vec<Label>, ParticipantError<P::Error>> {
        let mut labels = Vec::with_capacity(ids.len());
        for id in ids {
            let label = self.port.find_label(self.repo, *id).await.map_err(ParticipantError::Store)?;
            labels.push(label.ok_or_else(|| ParticipantError::Invalid("unknown label".to_owned()))?);
        }
        Ok(labels)
    }

    /// The active users with these ids, each able to push to this repository; anyone
    /// else is invalid. Checked in order, stopping at the first refusal.
    pub async fn assignees(&self, ids: &[AccountId]) -> Result<Vec<User>, ParticipantError<P::Error>> {
        let mut users = Vec::with_capacity(ids.len());
        for id in ids {
            let user = self.port.find_active_user(*id).await.map_err(ParticipantError::Store)?;
            let user = user.ok_or_else(|| ParticipantError::Invalid("unknown user".to_owned()))?;
            if !self.port.can_push(&user).await.map_err(ParticipantError::Store)? {
                return Err(ParticipantError::Invalid(format!(
                    "{} cannot be assigned: only people who can push to the repository can",
                    user.name()
                )));
            }
            users.push(user);
        }
        Ok(users)
    }

    /// People who may be assigned, for a viewer who `can_read` the repository: names
    /// starting with the trimmed `prefix`, at most [`MAX_ASSIGNABLE`].
    pub async fn assignable(&self, can_read: bool, prefix: &str) -> Result<Vec<Account>, ParticipantError<P::Error>> {
        if !can_read {
            return Err(ParticipantError::NotFound);
        }
        self.port.assignable(self.repo, prefix.trim(), MAX_ASSIGNABLE).await.map_err(ParticipantError::Store)
    }

    /// The number of the thread comment `id` belongs to.
    pub async fn comment_thread(&self, id: CommentId) -> Result<IssueNumber, ParticipantError<P::Error>> {
        self.port.comment_thread(self.repo, id).await.map_err(ParticipantError::Store)?.ok_or(ParticipantError::NotFound)
    }
}
