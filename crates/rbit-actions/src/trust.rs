//! Whether a run may touch what the repository protects (§11.1 of the Actions design):
//! pull request trust, what a stored run is granted, and where an untrusted run's shared
//! state lives.
//!
//! * A run is trusted only when the actor who caused the event can write to the base
//!   repository now and is not suspended ([`classify_pull_request`]). A fork's pull
//!   request is never trusted, whoever pushed last; a writer causing the event only skips
//!   the wait ([`PullRequestTrust`]).
//! * An untrusted run skips the approval wait only when its actor is the author and the
//!   author is vouched for. Approval never changes what a run is granted: a stored run's
//!   grants follow its stored trust alone ([`RunTrust::from_stored`], [`Grants`]), so an
//!   approved fork run still gets no secrets, no variables and a read-only token.
//! * An untrusted run's concurrency group and cache writes live under its pull request
//!   (or the run itself), so it can cancel, hold or poison nothing of anyone else
//!   ([`Isolation`]).
//!
//! The facts (access, suspension, trusted users, merged pull requests, the approval
//! policy) are read through [`TrustFacts`]; storage and identity stay in their adapters.

mod application;
mod ports;
#[cfg(test)]
mod tests;

use rbit_access::RepoAccess;

pub use application::PullRequestTrust;
pub use ports::{ActorFacts, TrustFacts};

/// The actor whose action caused a pull request event: the author for `opened`,
/// `reopened`, `ready_for_review` and `edited`, the pusher of the new head for
/// `synchronize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggeringActor {
    /// The pusher of the head is not known (or the head moved without a user push).
    Unknown,
    Known {
        /// The actor's access to the base repository now.
        access: RepoAccess,
        suspended: bool,
        /// The actor is the pull request's author.
        is_author: bool,
    },
}

/// What the base repository knows about a pull request's author.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorStanding {
    /// Nothing that vouches for them.
    Unknown,
    /// Listed in the repository's trusted users, or has a pull request merged into it.
    Vouched,
    /// Suspended: never approved automatically.
    Suspended,
}

/// How a pull request run may run (§11.1 of the design).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunTrust {
    /// Secrets, variables, and the repository's token permission.
    Trusted,
    /// A read-only token, no secrets and no variables — whoever approves it.
    Untrusted {
        /// The run waits for a writer's approval before any job is dispatched.
        awaits_approval: bool,
    },
}

impl RunTrust {
    pub const fn is_trusted(self) -> bool {
        matches!(self, Self::Trusted)
    }

    /// The trust of a stored run (its `trusted` column): whether it waited for approval no
    /// longer matters, and approving it changed nothing.
    pub const fn from_stored(trusted: bool) -> Self {
        if trusted { Self::Trusted } else { Self::Untrusted { awaits_approval: false } }
    }

    /// What tasks of a run with this trust receive.
    pub const fn grants(self) -> Grants {
        let trusted = self.is_trusted();
        Grants { secrets_and_variables: trusted, write_token: trusted }
    }
}

/// What the tasks of a run receive from the repository and its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grants {
    /// Repository, owner and environment secrets and variables (and their log masks).
    pub secrets_and_variables: bool,
    /// A task token above read: otherwise every permission is capped at read.
    pub write_token: bool,
}

impl Grants {
    /// What tasks of a stored run receive ([`RunTrust::from_stored`]).
    pub const fn of_stored(trusted: bool) -> Self {
        RunTrust::from_stored(trusted).grants()
    }
}

/// Classifies a `pull_request` run when it is created. Trusted only when the actor who
/// caused the event can write to the base repository now and is not suspended. An untrusted
/// run skips the approval wait only when the actor is the author and the author is vouched
/// for — approval never grants secrets.
pub fn classify_pull_request(actor: TriggeringActor, author: AuthorStanding) -> RunTrust {
    match actor {
        TriggeringActor::Known { access, suspended: false, .. } if access.can_write() => RunTrust::Trusted,
        TriggeringActor::Known { suspended: false, is_author: true, .. } => {
            RunTrust::Untrusted { awaits_approval: author != AuthorStanding::Vouched }
        }
        TriggeringActor::Known { .. } | TriggeringActor::Unknown => RunTrust::Untrusted { awaits_approval: true },
    }
}

/// Where a run's shared state (its concurrency group, the cache scope it writes) lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Isolation {
    /// A trusted run shares its ref's state.
    Shared,
    /// An untrusted run of pull request `n`: under `pr-<n>/` and `pull/<n>`.
    PullRequest(i64),
    /// An untrusted run of no pull request: under `untrusted/`.
    Untrusted,
}

impl Isolation {
    /// The isolation of a run with this trust, of pull request `pull_number` if any.
    pub const fn of(trusted: bool, pull_number: Option<i64>) -> Self {
        match (trusted, pull_number) {
            (true, _) => Self::Shared,
            (false, Some(number)) => Self::PullRequest(number),
            (false, None) => Self::Untrusted,
        }
    }

    /// The concurrency group a workflow's `group` becomes.
    pub fn namespaced_group(self, group: &str) -> String {
        match self {
            Self::Shared => group.to_owned(),
            Self::PullRequest(number) => format!("pr-{number}/{group}"),
            Self::Untrusted => format!("untrusted/{group}"),
        }
    }

    /// The cache scope a task writes: its own ref's scope (`own_ref`) when shared, else
    /// one no other run reads first; `run_id` names an untrusted run's own.
    pub fn cache_write_scope(self, own_ref: impl FnOnce() -> String, run_id: i64) -> String {
        match self {
            Self::Shared => own_ref(),
            Self::PullRequest(number) => format!("pull/{number}"),
            Self::Untrusted => format!("untrusted/{run_id}"),
        }
    }
}
