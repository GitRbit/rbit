//! Grants issued to running workflow tasks, shared with host authorization services.

use super::{JobId, PermissionLevel, RunId, TaskId, TokenPermissions};
use crate::{RepoAccess, RepoId};

/// What a running task's token grants, decided when it is presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskGrant {
    pub task: TaskId,
    pub job: JobId,
    pub run: RunId,
    /// The task's own repository: the only one it may write to.
    pub repo: RepoId,
    pub permissions: TokenPermissions,
}

impl TaskGrant {
    /// Access to its own repository's contents (Git, LFS, releases).
    pub fn contents(&self) -> RepoAccess {
        match self.permissions.contents {
            PermissionLevel::None => RepoAccess::None,
            PermissionLevel::Read => RepoAccess::Read,
            PermissionLevel::Write => RepoAccess::Write,
        }
    }

    /// Whether it may comment on some of its own repository's threads: an endpoint that
    /// comments asks no less of a task's token.
    pub fn can_comment(&self) -> bool {
        self.permissions.issues == PermissionLevel::Write || self.permissions.pull_requests == PermissionLevel::Write
    }

    // Actions: task tokens.
    /// Whether it may read issues and pull requests and their comments: `issues` or
    /// `pull-requests` at `read` or more.
    pub fn can_read_issues(&self) -> bool {
        self.permissions.issues >= PermissionLevel::Read || self.permissions.pull_requests >= PermissionLevel::Read
    }

    // Actions: task tokens.
    /// Whether it may comment on a thread of `kind`: an issue needs `issues: write`, a pull
    /// request `pull-requests: write` or `issues: write` (its conversation is an issue's).
    pub fn can_comment_on(&self, kind: crate::IssueKind) -> bool {
        match kind {
            crate::IssueKind::Issue => self.permissions.issues == PermissionLevel::Write,
            crate::IssueKind::Pull => self.can_comment(),
        }
    }
}
