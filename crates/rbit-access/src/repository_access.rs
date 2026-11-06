//! The permission lattice for repositories.

/// What an actor may do with a repository. Variants are ordered, so
/// `access >= RepoAccess::Write` reads naturally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum RepoAccess {
    /// The repository must appear not to exist (private repositories answer 404).
    #[default]
    None,
    /// Browse, clone and fetch.
    Read,
    /// Push to branches.
    Write,
    /// Change settings, rename, delete.
    Admin,
}

impl RepoAccess {
    /// Whether this level permits browsing, clone and fetch.
    pub fn can_read(self) -> bool {
        self >= Self::Read
    }

    /// Whether this level reaches the write threshold; operation gates still apply.
    pub fn can_write(self) -> bool {
        self >= Self::Write
    }

    /// Whether this level permits repository administration.
    pub fn can_admin(self) -> bool {
        self >= Self::Admin
    }
}
