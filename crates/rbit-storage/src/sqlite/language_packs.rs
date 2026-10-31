//! Bounded language-pack archives and independent account locale preferences.

use rbit_core::{AccountId, Timestamp};

use super::pool::Pool;
use crate::StorageError;

/// Maximum compressed bytes per installed language pack.
pub const MAX_LANGUAGE_PACK_BYTES: usize = 2 * 1024 * 1024;
/// Maximum installed language packs, including packs whose provider is disabled.
pub const MAX_LANGUAGE_PACKS: usize = 64;

/// An installed archive and its validated metadata, included in SQLite backups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguagePack {
    pub id: String,
    pub locale: String,
    pub version: String,
    pub bytes: Vec<u8>,
    pub digest: String,
    pub installed_by: Option<AccountId>,
    pub updated_at: Timestamp,
}

/// Administrator inventory without loading compressed archives into a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguagePackInfo {
    pub id: String,
    pub locale: String,
    pub version: String,
    pub digest: String,
    pub bytes: usize,
    pub message_count: usize,
    pub message_total: usize,
    pub updated_at: Timestamp,
}

#[derive(Debug, sqlx::FromRow)]
struct MetadataRow {
    id: String,
    locale: String,
    version: String,
    digest: String,
    bytes: i64,
    message_count: i64,
    message_total: i64,
    updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct LanguagePackRow {
    id: String,
    locale: String,
    version: String,
    bytes: Vec<u8>,
    digest: String,
    installed_by: Option<i64>,
    updated_at: i64,
}

impl From<LanguagePackRow> for LanguagePack {
    fn from(row: LanguagePackRow) -> Self {
        Self {
            id: row.id,
            locale: row.locale,
            version: row.version,
            bytes: row.bytes,
            digest: row.digest,
            installed_by: row.installed_by.map(AccountId),
            updated_at: Timestamp(row.updated_at),
        }
    }
}

/// Validated package data to install or atomically replace under the same ID.
#[derive(Debug)]
pub struct NewLanguagePack<'a> {
    pub id: &'a str,
    pub locale: &'a str,
    pub version: &'a str,
    pub bytes: &'a [u8],
    pub digest: &'a str,
    pub message_count: usize,
    pub message_total: usize,
    pub installed_by: Option<AccountId>,
    pub updated_at: Timestamp,
}

/// Host-owned language-pack and locale-preference persistence.
#[derive(Debug, Clone)]
pub struct LanguagePackStore {
    pool: Pool,
}

impl LanguagePackStore {
    pub(crate) fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Reads all installed archives once during catalog preparation, never per render.
    pub async fn list(&self) -> Result<Vec<LanguagePack>, StorageError> {
        let rows: Vec<LanguagePackRow> =
            sqlx::query_as("SELECT id, locale, version, bytes, digest, installed_by, updated_at FROM language_packs ORDER BY id")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn get(&self, id: &str) -> Result<Option<LanguagePack>, StorageError> {
        let row: Option<LanguagePackRow> =
            sqlx::query_as("SELECT id, locale, version, bytes, digest, installed_by, updated_at FROM language_packs WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(Into::into))
    }

    pub async fn metadata(&self) -> Result<Vec<LanguagePackInfo>, StorageError> {
        let rows: Vec<MetadataRow> =
            sqlx::query_as("SELECT id, locale, version, digest, length(bytes) AS bytes, message_count, message_total, updated_at FROM language_packs ORDER BY id")
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter()
            .map(|row| {
                Ok(LanguagePackInfo {
                    id: row.id,
                    locale: row.locale,
                    version: row.version,
                    digest: row.digest,
                    bytes: usize::try_from(row.bytes).map_err(|_| StorageError::Corrupt("invalid language-pack byte count".to_owned()))?,
                    message_count: usize::try_from(row.message_count)
                        .map_err(|_| StorageError::Corrupt("invalid language-pack message count".to_owned()))?,
                    message_total: usize::try_from(row.message_total)
                        .map_err(|_| StorageError::Corrupt("invalid language-pack schema count".to_owned()))?,
                    updated_at: Timestamp(row.updated_at),
                })
            })
            .collect()
    }

    /// Writes a complete archive atomically; `false` means the installation cap was hit.
    /// Updating an existing ID remains permitted at capacity. SQLite also enforces the
    /// cap and byte limit against writers that bypass this trusted store API.
    pub async fn put(&self, pack: &NewLanguagePack<'_>) -> Result<bool, StorageError> {
        let mut transaction = self.pool.begin_immediate().await?;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM language_packs WHERE id = ?)")
            .bind(pack.id)
            .fetch_one(&mut *transaction)
            .await?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM language_packs").fetch_one(&mut *transaction).await?;
        if !exists && count >= 64 {
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO language_packs (id, locale, version, bytes, digest, installed_by, updated_at, message_count, message_total) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) ON CONFLICT(id) DO UPDATE SET \
             locale = excluded.locale, version = excluded.version, bytes = excluded.bytes, \
             digest = excluded.digest, installed_by = excluded.installed_by, updated_at = excluded.updated_at, \
             message_count = excluded.message_count, message_total = excluded.message_total",
        )
        .bind(pack.id)
        .bind(pack.locale)
        .bind(pack.version)
        .bind(pack.bytes)
        .bind(pack.digest)
        .bind(pack.installed_by.map(|actor| actor.0))
        .bind(pack.updated_at.0)
        .bind(i64::try_from(pack.message_count).map_err(|_| StorageError::Corrupt("language-pack message count overflow".to_owned()))?)
        .bind(i64::try_from(pack.message_total).map_err(|_| StorageError::Corrupt("language-pack schema count overflow".to_owned()))?)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(true)
    }

    pub async fn remove(&self, id: &str) -> Result<bool, StorageError> {
        Ok(sqlx::query("DELETE FROM language_packs WHERE id = ?").bind(id).execute(&self.pool).await?.rows_affected() != 0)
    }

    pub async fn account_locale(&self, account: AccountId) -> Result<Option<String>, StorageError> {
        Ok(sqlx::query_scalar("SELECT locale FROM account_locales WHERE account_id = ?").bind(account.0).fetch_optional(&self.pool).await?)
    }

    /// Stores a user preference independently of installed packages or provider state.
    pub async fn set_account_locale(&self, account: AccountId, locale: Option<&str>) -> Result<(), StorageError> {
        match locale {
            Some(locale) => {
                sqlx::query(
                    "INSERT INTO account_locales (account_id, locale) VALUES (?1, ?2) \
                     ON CONFLICT(account_id) DO UPDATE SET locale = excluded.locale",
                )
                .bind(account.0)
                .bind(locale)
                .execute(&self.pool)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM account_locales WHERE account_id = ?").bind(account.0).execute(&self.pool).await?;
            }
        }
        Ok(())
    }
}
