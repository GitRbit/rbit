//! Metadata for a bare Git repository, independent of persistence and Git I/O.

use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::Account;

use crate::{RepoName, Visibility};

#[derive(Debug, Clone)]
pub struct Repository {
    pub id: RepoId,
    pub owner_id: AccountId,
    pub name: RepoName,
    pub description: Option<String>,
    pub visibility: Visibility,
    pub default_branch: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub pushed_at: Option<Timestamp>,
    /// When the repository was archived; `None` while it accepts changes.
    pub archived_at: Option<Timestamp>,
}

/// Why an archived repository refuses a change, shown to people and Git clients alike.
pub const ARCHIVED_READ_ONLY: &str = "This repository is archived and read-only; an administrator can unarchive it in its settings";

impl Repository {
    /// Archived repositories are read-only: they can be read, cloned, forked and starred,
    /// but nothing in them changes until they are unarchived.
    pub const fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

/// A repository together with its owner — the shape almost every page needs.
#[derive(Debug, Clone)]
pub struct OwnedRepository {
    pub owner: Account,
    pub repo: Repository,
}

impl OwnedRepository {
    /// `owner/name`, as shown in headings and used in URLs.
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner.name, self.repo.name)
    }
}
