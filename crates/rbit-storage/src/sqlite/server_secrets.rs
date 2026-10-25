//! Keys the server generates for itself (`server_secrets`, migration
//! `0034_notification_email.sql`).
//!
//! Milestone 3 workstream N (email notifications) added this store; any feature that
//! needs a stable server-side key (HMAC-signed links, for instance) can keep one here
//! under its own name. Values are raw key material: never log them or send them anywhere.
//! They are stored [sealed](crate::sealing).

use rbit_core::Timestamp;
use sqlx::SqlitePool;

use crate::{StorageError, sealing::SealingKey};

#[derive(Debug, Clone)]
pub struct ServerSecretStore {
    pool: SqlitePool,
    key: SealingKey,
}

impl ServerSecretStore {
    pub(crate) fn new(pool: SqlitePool, key: SealingKey) -> Self {
        Self { pool, key }
    }

    /// The secret stored as `name`, storing `fresh` first if there is none yet. Concurrent
    /// callers all get the one value that won.
    pub async fn get_or_insert(&self, name: &str, fresh: &[u8]) -> Result<Vec<u8>, StorageError> {
        sqlx::query("INSERT INTO server_secrets (name, secret, created_at) VALUES (?, ?, ?) ON CONFLICT (name) DO NOTHING")
            .bind(name)
            .bind(self.key.seal(fresh)?.into_bytes())
            .bind(Timestamp::now().seconds())
            .execute(&self.pool)
            .await?;
        let stored: Vec<u8> =
            sqlx::query_scalar("SELECT secret FROM server_secrets WHERE name = ?").bind(name).fetch_one(&self.pool).await?;
        Ok(self.key.open(&stored)?)
    }
}

#[cfg(test)]
mod tests {
    use crate::Database;

    #[tokio::test]
    async fn the_first_secret_stays() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let store = database.server_secrets();
        assert_eq!(store.get_or_insert("links", &[1, 2, 3]).await.unwrap(), [1, 2, 3]);
        assert_eq!(store.get_or_insert("links", &[9, 9]).await.unwrap(), [1, 2, 3], "an existing secret is never replaced");
        assert_eq!(store.get_or_insert("other", &[7]).await.unwrap(), [7]);
    }
}
