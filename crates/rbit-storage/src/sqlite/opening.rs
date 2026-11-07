//! Shared key handling AFTER the relevant legacy or paired acceptance boundary.
#[cfg(test)]
use super::Database;
use super::reseal;
use crate::{StorageError, sealing::SealingKey};
use sqlx::SqlitePool;
use std::path::Path;
pub(super) async fn key(pool: &SqlitePool, key_file: &Path) -> Result<SealingKey, StorageError> {
    let key = match SealingKey::load(key_file)? {
        Some(key) => key,
        None if reseal::sample(pool).await?.is_some() => return Err(StorageError::MissingKey(key_file.display().to_string())),
        None => SealingKey::create(key_file)?,
    };
    reseal::check_key(pool, &key, key_file).await?;
    reseal::seal_plaintext(pool, &key).await?;
    Ok(key)
}
#[cfg(test)]
impl Database {
    pub(crate) async fn from_accepted(pool: crate::identity_upgrade::active::ActiveSqlPool, key_file: &Path) -> Result<Self, StorageError> {
        let (pool, protocol) = pool.into_storage_parts();
        let key = key(&pool, key_file).await?;
        let database = Self { pool, key, protocol };
        database.optimize().await?;
        Ok(database)
    }
}
