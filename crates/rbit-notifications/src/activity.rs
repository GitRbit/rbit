//! Feature-owned activity input; composition maps domain events and drops the rest.
use rbit_collaboration::{
    comments::CommentId,
    threads::{IssueId, IssueText},
};
use rbit_contracts::{AccountId, RepoId};

/// What happened on a thread, already reduced to the cases that can notify anyone.
/// Unrelated events (title-only edits, deleted comments, other actions) never reach the
/// fan-out; composition drops them before any storage work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    /// A thread was opened: its author takes part; assignees and new mentions hear of it.
    Opened,
    /// The thread's body changed: newly mentioned users hear of it.
    BodyEdited,
    Assigned(AccountId),
    ReviewRequested(AccountId),
    /// Closed or reopened: every subscriber hears of it.
    StateChanged,
    /// A comment was added: its author takes part; subscribers and new mentions hear of it.
    CommentCreated(CommentId),
    /// A comment changed: only newly mentioned users hear of it.
    CommentEdited(CommentId),
    /// A pull request review was submitted: its author takes part; subscribers hear of it.
    ReviewSubmitted,
}

/// One activity on `issue` of `repo` by `actor` (`None` for an automated task).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivityEvent {
    pub repo: RepoId,
    pub issue: IssueId,
    pub actor: Option<AccountId>,
    pub activity: Activity,
}

/// The parts of a thread the fan-out reads: who opened it and its Markdown body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadText {
    pub author: Option<AccountId>,
    pub body: IssueText,
}

/// An existing account named by a Markdown mention. Organizations are never notified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MentionedAccount {
    pub id: AccountId,
    pub organization: bool,
}
