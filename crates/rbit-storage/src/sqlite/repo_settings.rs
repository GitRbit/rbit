//! Repository metadata updates: description, visibility, default branch, rename, archive,
//! delete.
//!
//! Workstream D owns this file. Keep row types here rather than in `rows.rs`.

use super::pool::Pool;
use rbit_core::{AccountId, RepoId, RepoName, Timestamp, Visibility};

use crate::StorageError;

/// The general settings of a repository, written together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoGeneralSettings {
    pub description: Option<String>,
    pub visibility: Visibility,
    pub default_branch: String,
}

#[derive(Debug, Clone)]
pub struct RepoSettingsStore {
    pool: Pool,
}

impl RepoSettingsStore {
    pub(crate) fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn update_general(&self, id: RepoId, settings: &RepoGeneralSettings) -> Result<(), StorageError> {
        super::repository_writes::update_general(&self.pool, id, settings).await
    }

    /// [`StorageError::Conflict`] (`"repository name"`) if the owner already has a repository
    /// called `name` (case-insensitively).
    pub async fn rename(&self, id: RepoId, name: &RepoName) -> Result<(), StorageError> {
        super::repository_writes::rename(&self.pool, id, name).await
    }

    /// Marks the repository archived by `by` at `at`. `false` when it already was (the
    /// original time and actor are kept) or does not exist.
    pub async fn archive(&self, id: RepoId, by: AccountId, at: Timestamp) -> Result<bool, StorageError> {
        super::repository_writes::archive(&self.pool, id, by, at).await
    }

    /// Makes the repository writable again. `false` when it was not archived.
    pub async fn unarchive(&self, id: RepoId) -> Result<bool, StorageError> {
        super::repository_writes::unarchive(&self.pool, id).await
    }

    /// Removes the metadata row (stars cascade). The caller deletes the bare repository.
    pub async fn delete(&self, id: RepoId) -> Result<(), StorageError> {
        super::repository_writes::delete(&self.pool, id).await
    }
}
