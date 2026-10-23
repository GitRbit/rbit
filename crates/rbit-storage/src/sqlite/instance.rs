//! Administrator overrides of instance settings (`instance_settings` key/value table).
//!
//! The store is deliberately schema-less: it persists opaque key/value pairs and leaves
//! parsing (and falling back to configured defaults) to the instance service, so adding a
//! setting never needs a migration.

use rbit_core::Timestamp;
use sqlx::SqlitePool;

use crate::StorageError;

/// The administrator-saved deployment configuration (a JSON document). Its credential
/// fields are [sealed](crate::sealing::SealingKey::seal_secret_fields) by the caller.
pub const DEPLOYMENT_CONFIG_KEY: &str = "deployment_config";

#[derive(Debug, Clone)]
pub struct InstanceSettingsStore {
    pool: SqlitePool,
}

impl InstanceSettingsStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Every stored override as `(key, value)`, ordered by key.
    pub async fn load(&self) -> Result<Vec<(String, String)>, StorageError> {
        Ok(sqlx::query_as("SELECT key, value FROM instance_settings ORDER BY key").fetch_all(&self.pool).await?)
    }

    /// Removes an override so deployment defaults apply on the next restart.
    pub async fn delete(&self, key: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM instance_settings WHERE key = ?").bind(key).execute(&self.pool).await?;
        Ok(())
    }

    /// Inserts or replaces all `entries` atomically.
    pub async fn save(&self, entries: &[(&str, &str)]) -> Result<(), StorageError> {
        let now = Timestamp::now().seconds();
        let mut tx = self.pool.begin().await?;
        for (key, value) in entries {
            sqlx::query(
                "INSERT INTO instance_settings (key, value, updated_at) VALUES (?, ?, ?) \
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            )
            .bind(key)
            .bind(value)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Database;

    #[tokio::test]
    async fn saves_and_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let store = database.instance_settings();
        assert_eq!(store.load().await.unwrap(), Vec::<(String, String)>::new());

        store.save(&[("site_name", "Forge"), ("registration", "closed")]).await.unwrap();
        store.save(&[("site_name", "Code")]).await.unwrap();

        let stored = store.load().await.unwrap();
        assert_eq!(stored, [("registration".to_owned(), "closed".to_owned()), ("site_name".to_owned(), "Code".to_owned())]);
    }
}
