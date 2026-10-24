//! Repository metadata updates: description, visibility, default branch, rename, delete.
//!
//! Workstream D owns this file. Keep row types here rather than in `rows.rs`.

use super::pool::Pool;
use rbit_core::{RepoId, RepoName, Timestamp, Visibility};

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
        sqlx::query("UPDATE repositories SET description = ?, visibility = ?, default_branch = ?, updated_at = ? WHERE id = ?")
            .bind(settings.description.as_deref())
            .bind(settings.visibility.as_str())
            .bind(&settings.default_branch)
            .bind(Timestamp::now().seconds())
            .bind(id.0)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// [`StorageError::Conflict`] (`"repository name"`) if the owner already has a repository
    /// called `name` (case-insensitively).
    pub async fn rename(&self, id: RepoId, name: &RepoName) -> Result<(), StorageError> {
        sqlx::query("UPDATE repositories SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name.as_str())
            .bind(Timestamp::now().seconds())
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::from_insert(e, "repository name"))?;
        Ok(())
    }

    /// Removes the metadata row (stars cascade). The caller deletes the bare repository.
    pub async fn delete(&self, id: RepoId) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM repositories WHERE id = ?").bind(id.0).execute(&self.pool).await?;
        Ok(())
    }
}
