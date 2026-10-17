//! One ref changed (or to be changed) by a push.
//!
//! Used for what a push did (`rustit_git::push` compares snapshots) and for what a client
//! asks a push to do (the push gate reads the command list before git does).

/// One ref changed by a push. `before: None` means the ref was created, `after: None`
/// that it was deleted. Ids are full hexadecimal object ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefUpdate {
    /// Full ref name, e.g. `refs/heads/main` or `refs/tags/v1.0`.
    pub name: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

impl RefUpdate {
    pub fn is_created(&self) -> bool {
        self.before.is_none()
    }

    pub fn is_deleted(&self) -> bool {
        self.after.is_none()
    }

    /// The branch name, when this updates `refs/heads/…`.
    pub fn branch(&self) -> Option<&str> {
        self.name.strip_prefix("refs/heads/")
    }

    /// The tag name, when this updates `refs/tags/…`.
    pub fn tag(&self) -> Option<&str> {
        self.name.strip_prefix("refs/tags/")
    }
}
