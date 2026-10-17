//! Branch protection: rules per branch pattern and the pure decisions made from them —
//! which ref updates a push may make ([`PushVerdict`]) and what merging a pull request
//! into a branch requires ([`MergeRules`]).
//!
//! Milestone 2 workstream B (branch protection) owns this file. The types below are the
//! shared contract of `docs/design/milestone-2b.md` §3; the pattern matching and the
//! decision functions are B's to add.

use crate::{RepoAccess, User};

/// One ref update a push may not make, and why. `reason` is shown to the client
/// (`! [remote rejected] main -> main (<reason>)`) and never lists who is allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The full ref name, e.g. `refs/heads/main`.
    pub name: String,
    pub reason: String,
}

/// Who is pushing.
#[derive(Debug, Clone, Copy)]
pub struct PushActor<'a> {
    /// The signed-in user; `None` for deploy keys.
    pub user: Option<&'a User>,
    /// Whether the push is made with a deploy key.
    pub deploy_key: bool,
    /// The pusher's access to the repository, after the token scope.
    pub access: RepoAccess,
}

/// What the push gate decided before git sees the push.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PushVerdict {
    /// Every refused ref; when not empty, the whole push is refused.
    pub refusals: Vec<Refusal>,
    /// Run `git receive-pack` with `receive.denyNonFastForwards`: some update of an
    /// existing branch is protected against force pushes for this pusher.
    pub deny_non_fast_forwards: bool,
}

impl PushVerdict {
    /// Everything is allowed.
    pub fn allow() -> Self {
        Self::default()
    }

    pub fn is_refused(&self) -> bool {
        !self.refusals.is_empty()
    }
}

/// What merging a pull request into a branch requires of the merger.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "independent rule flags")]
pub struct MergeRules {
    /// The patterns of the rules that match the branch, for display.
    pub patterns: Vec<String>,
    pub require_pull_request: bool,
    pub required_approvals: u8,
    pub dismiss_stale_approvals: bool,
    pub block_on_changes_requested: bool,
    /// `restrict_pushers` allows the merger (always true without restrictions).
    pub merger_may_push: bool,
    /// The merger is an administrator and a matching rule allows administrators to bypass.
    pub merger_may_bypass: bool,
}

impl MergeRules {
    /// No rule matches the branch.
    pub fn unprotected() -> Self {
        Self {
            patterns: Vec::new(),
            require_pull_request: false,
            required_approvals: 0,
            dismiss_stale_approvals: false,
            block_on_changes_requested: false,
            merger_may_push: true,
            merger_may_bypass: false,
        }
    }

    pub fn is_protected(&self) -> bool {
        !self.patterns.is_empty()
    }
}
