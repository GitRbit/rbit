//! Instrument the enabled-repository gate used before log-page reads.
use super::ActionsSettingsStore;
use crate::{StorageError, sqlite::pool::Pool};
use rbit_core::RepoId;

impl ActionsSettingsStore {
    /// Whether workflows run in the repository: it turned Actions on, and its owner lets
    /// it (every repository, or the selected ones with it among them).
    pub async fn is_enabled(&self, repo: RepoId) -> Result<bool, StorageError> {
        let pool = Pool::new(self.pool.clone(), "actions_settings");
        let enabled: Option<bool> = sqlx::query_scalar(
            "SELECT a.enabled = 1 AND CASE COALESCE(o.enabled_repositories, 'all') \
                 WHEN 'all' THEN 1 \
                 WHEN 'selected' THEN EXISTS (SELECT 1 FROM actions_owner_enabled_repositories e \
                                              WHERE e.owner_id = r.owner_id AND e.repo_id = r.id) \
                 ELSE 0 END \
             FROM actions_repositories a JOIN repositories r ON r.id = a.repo_id \
             LEFT JOIN actions_owner_settings o ON o.owner_id = r.owner_id WHERE a.repo_id = ?",
        )
        .bind(repo.0)
        .fetch_optional(&pool)
        .await?;
        Ok(enabled.unwrap_or(false))
    }
}
