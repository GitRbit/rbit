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
    pub fn can_read(self) -> bool {
        self >= Self::Read
    }

    pub fn can_write(self) -> bool {
        self >= Self::Write
    }

    pub fn can_admin(self) -> bool {
        self >= Self::Admin
    }
}
