//! Where a push came from: who (or what) is accountable for a set of ref updates.
//!
//! Every [`RefUpdate`](crate::RefUpdate) reported after a push carries one of these
//! (`DomainEvent::RepositoryPushed::source`). An account alone cannot tell the cases
//! apart: a deploy key, a mirror fetch and a workflow task all push without one, and
//! the Actions trust rules (`docs/design/actions.md` §11.2) treat them differently.

use crate::{AccountId, actions::TaskId};

/// Who is accountable for a push.
///
/// Changes a user asked for in the browser are that user's: a merge, a file edit, a
/// release that creates its tag, deleting a merged head branch. Only changes nobody
/// asked for at that moment are [`PushSource::System`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PushSource {
    /// A signed-in account: Git over HTTP or SSH with the user's own credentials, or an
    /// action the user took in the browser.
    User(AccountId),
    /// A writable deploy key over SSH. It belongs to the repository, not to an account.
    DeployKey,
    /// A pull mirror fetching its upstream.
    Mirror,
    /// Copying refs while importing a repository. Imports publish no push today
    /// (`imports/job.rs`); one that does must use this source.
    Import,
    /// A workflow task pushing with its task token (`docs/design/actions.md` §11.3).
    Task(TaskId),
    /// rbit itself, with nobody accountable.
    System,
}

impl PushSource {
    /// The account that pushed: only [`PushSource::User`] has one.
    pub fn pusher(self) -> Option<AccountId> {
        match self {
            Self::User(account) => Some(account),
            Self::DeployKey | Self::Mirror | Self::Import | Self::Task(_) | Self::System => None,
        }
    }

    /// Whether a push from here may start workflow runs, including the `synchronize` of
    /// a pull request whose head it moved (§11.2, §11.5): only people and their deploy
    /// keys do. Mirrors, imports, tasks and rbit itself never start runs, so a task
    /// cannot trigger itself and an upstream's history cannot run code here.
    pub fn starts_runs(self) -> bool {
        matches!(self, Self::User(_) | Self::DeployKey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [PushSource; 6] = [
        PushSource::User(AccountId(7)),
        PushSource::DeployKey,
        PushSource::Mirror,
        PushSource::Import,
        PushSource::Task(TaskId(3)),
        PushSource::System,
    ];

    #[test]
    fn only_users_have_a_pusher() {
        let pushers: Vec<_> = ALL.iter().map(|source| source.pusher()).collect();
        assert_eq!(pushers, [Some(AccountId(7)), None, None, None, None, None]);
    }

    #[test]
    fn only_users_and_deploy_keys_start_runs() {
        let starting: Vec<_> = ALL.into_iter().filter(|source| source.starts_runs()).collect();
        assert_eq!(starting, [PushSource::User(AccountId(7)), PushSource::DeployKey]);
    }
}
