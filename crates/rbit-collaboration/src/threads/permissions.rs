//! Who may do what with a thread and its comments: pure tables over the viewer, their
//! repository access and the row. Archived repositories offer nothing.

use rbit_access::RepoAccess;
use rbit_identity_models::User;

use super::{Issue, IssueKind};
use crate::comments::Comment;

/// What a viewer may do with an issue or pull request. Reading is not listed: whoever can
/// read the repository can read every thread in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[expect(clippy::struct_excessive_bools, reason = "independent permission flags")]
pub struct IssuePermissions {
    /// Add a comment (signed in; on a locked thread only writers).
    pub can_comment: bool,
    /// Edit the title and body.
    pub can_edit: bool,
    /// Close and reopen.
    pub can_set_state: bool,
    pub can_lock: bool,
    /// Labels, milestones, assignees, reviewers.
    pub can_triage: bool,
    /// Delete the whole thread (issues only).
    pub can_delete: bool,
    /// Delete other people's comments.
    pub can_moderate: bool,
}

/// What `viewer` may do with `issue`, given their `access` to its repository (token
/// scopes already folded in by the caller with `TokenScope::limit`).
///
/// | Action | Who |
/// |---|---|
/// | comment | signed in, `access ≥ Read`; on a locked thread `access ≥ Write` |
/// | edit title/body, close/reopen | the author (with `access ≥ Read`), or `access ≥ Write` |
/// | lock/unlock, triage, moderate comments | `access ≥ Write` |
/// | delete | `access ≥ Admin`, issues only (pull requests cannot be deleted) |
///
/// Anonymous viewers and viewers without read access may do nothing.
pub fn issue_permissions(viewer: Option<&User>, access: RepoAccess, issue: &Issue) -> IssuePermissions {
    let Some(viewer) = viewer else { return IssuePermissions::default() };
    if !access.can_read() {
        return IssuePermissions::default();
    }
    let writer = access.can_write();
    let author = issue.is_author(viewer);
    IssuePermissions {
        can_comment: writer || !issue.locked,
        can_edit: writer || author,
        can_set_state: writer || author,
        can_lock: writer,
        can_triage: writer,
        can_delete: access.can_admin() && issue.kind == IssueKind::Issue,
        can_moderate: writer,
    }
}

/// What a viewer may do with one comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommentPermissions {
    /// Its author, while they may still comment on the thread.
    pub can_edit: bool,
    /// Its author, or anyone who may moderate the thread.
    pub can_delete: bool,
}

/// What `viewer` may do with `comment` on a thread they hold `permissions` for (from
/// [`issue_permissions`]).
pub fn comment_permissions(viewer: Option<&User>, permissions: IssuePermissions, comment: &Comment) -> CommentPermissions {
    let own = viewer.is_some_and(|viewer| comment.author == Some(viewer.id()));
    CommentPermissions {
        can_edit: own && permissions.can_comment,
        can_delete: (own && permissions.can_comment) || permissions.can_moderate,
    }
}

/// What `viewer` is offered on `issue` of a repository they hold `access` to:
/// [`issue_permissions`], or nothing at all while the repository is `archived` (it is
/// read-only).
pub fn thread_permissions(viewer: Option<&User>, access: RepoAccess, archived: bool, issue: &Issue) -> IssuePermissions {
    if archived { IssuePermissions::default() } else { issue_permissions(viewer, access, issue) }
}
