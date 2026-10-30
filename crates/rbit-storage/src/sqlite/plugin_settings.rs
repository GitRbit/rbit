//! Persisted plugin intent and recovery markers for interrupted disable operations.

use rbit_core::{AccountId, Timestamp};

use super::pool::Pool;
use crate::StorageError;

/// Host-owned plugin state. Package code cannot modify its own approved capabilities.
#[derive(Debug, Clone)]
pub struct PluginSettingsStore {
    pool: Pool,
}

impl PluginSettingsStore {
    pub(crate) fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Explicit administrator intent, or no override of the deployment startup default.
    pub async fn desired_enabled(&self, id: &str) -> Result<Option<bool>, StorageError> {
        Ok(sqlx::query_scalar("SELECT enabled FROM plugin_settings WHERE plugin_id = ?").bind(id).fetch_optional(&self.pool).await?)
    }

    /// Records intent before lifecycle work. Enabling never clears incomplete revocation.
    pub async fn set_desired_enabled(&self, id: &str, enabled: bool, actor: Option<AccountId>, now: Timestamp) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO plugin_settings (plugin_id, enabled, revocation_pending, updated_by, updated_at) \
             VALUES (?1, ?2, NOT ?2, ?3, ?4) ON CONFLICT(plugin_id) DO UPDATE SET \
             enabled = excluded.enabled, revocation_pending = CASE WHEN excluded.enabled = 0 THEN 1 \
             ELSE plugin_settings.revocation_pending END, updated_by = excluded.updated_by, updated_at = excluded.updated_at",
        )
        .bind(id)
        .bind(enabled)
        .bind(actor.map(|actor| actor.0))
        .bind(now.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Whether a prior disable must be reconciled before allowing a fresh generation.
    pub async fn disable_pending(&self, id: &str) -> Result<bool, StorageError> {
        Ok(sqlx::query_scalar("SELECT revocation_pending FROM plugin_settings WHERE plugin_id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .unwrap_or(false))
    }

    /// Called only after task credentials and unfinished work have been reconciled.
    pub async fn complete_disable(&self, id: &str) -> Result<(), StorageError> {
        sqlx::query("UPDATE plugin_settings SET revocation_pending = 0 WHERE plugin_id = ?").bind(id).execute(&self.pool).await?;
        Ok(())
    }
}
