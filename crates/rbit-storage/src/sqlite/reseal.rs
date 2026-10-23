//! Start-up housekeeping for [sealed](crate::sealing) credentials: finding out whether
//! any exist, checking that the key opens them, and sealing values that are still stored
//! in plaintext (written before sealing existed, or restored from an older backup).
//!
//! Sealing happens here rather than in a migration because the key never enters the
//! database. It is idempotent: sealed values are recognised by their prefix and skipped.

use std::path::Path;

use serde_json::Value;
use sqlx::SqlitePool;

use super::instance::DEPLOYMENT_CONFIG_KEY;
use crate::{
    StorageError,
    sealing::{PREFIX, SealingKey, is_sealed, is_secret_field},
};

/// One sealed value from anywhere in the database, if there is any.
pub(super) async fn sample(pool: &SqlitePool) -> Result<Option<Vec<u8>>, StorageError> {
    let sealed: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT secret FROM two_factor WHERE substr(secret, 1, 7) = CAST(?1 AS BLOB) \
         UNION ALL SELECT secret FROM server_secrets WHERE substr(secret, 1, 7) = CAST(?1 AS BLOB) \
         UNION ALL SELECT CAST(secret AS BLOB) FROM webhooks WHERE substr(secret, 1, 7) = ?1 \
         UNION ALL SELECT CAST(secret AS BLOB) FROM repo_push_mirrors WHERE substr(secret, 1, 7) = ?1 \
         LIMIT 1",
    )
    .bind(PREFIX)
    .fetch_optional(pool)
    .await?;
    if sealed.is_some() {
        return Ok(sealed);
    }
    let document: Option<String> =
        sqlx::query_scalar("SELECT value FROM instance_settings WHERE key = ?").bind(DEPLOYMENT_CONFIG_KEY).fetch_optional(pool).await?;
    Ok(document.and_then(|document| serde_json::from_str(&document).ok()).as_ref().and_then(first_sealed_field).map(String::into_bytes))
}

/// Fails with [`StorageError::WrongKey`] if `key` cannot open what is sealed in the database.
pub(super) async fn check_key(pool: &SqlitePool, key: &SealingKey, key_file: &Path) -> Result<(), StorageError> {
    if let Some(sealed) = sample(pool).await?
        && key.open(&sealed).is_err()
    {
        return Err(StorageError::WrongKey(key_file.display().to_string()));
    }
    Ok(())
}

/// Seals every credential still stored in plaintext, in one transaction.
pub(super) async fn seal_plaintext(pool: &SqlitePool, key: &SealingKey) -> Result<(), StorageError> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let mut sealed = 0_usize;

    let rows: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT account_id, secret FROM two_factor WHERE substr(secret, 1, 7) <> CAST(? AS BLOB)")
            .bind(PREFIX)
            .fetch_all(&mut *tx)
            .await?;
    for (account, secret) in rows {
        sqlx::query("UPDATE two_factor SET secret = ? WHERE account_id = ?")
            .bind(key.seal(&secret)?.into_bytes())
            .bind(account)
            .execute(&mut *tx)
            .await?;
        sealed += 1;
    }

    let rows: Vec<(String, Vec<u8>)> =
        sqlx::query_as("SELECT name, secret FROM server_secrets WHERE substr(secret, 1, 7) <> CAST(? AS BLOB)")
            .bind(PREFIX)
            .fetch_all(&mut *tx)
            .await?;
    for (name, secret) in rows {
        sqlx::query("UPDATE server_secrets SET secret = ? WHERE name = ?")
            .bind(key.seal(&secret)?.into_bytes())
            .bind(name)
            .execute(&mut *tx)
            .await?;
        sealed += 1;
    }

    let rows: Vec<(i64, String)> = sqlx::query_as("SELECT id, secret FROM webhooks WHERE secret IS NOT NULL AND substr(secret, 1, 7) <> ?")
        .bind(PREFIX)
        .fetch_all(&mut *tx)
        .await?;
    for (id, secret) in rows {
        sqlx::query("UPDATE webhooks SET secret = ? WHERE id = ?").bind(key.seal(secret.as_bytes())?).bind(id).execute(&mut *tx).await?;
        sealed += 1;
    }

    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, secret FROM repo_push_mirrors WHERE secret IS NOT NULL AND substr(secret, 1, 7) <> ?")
            .bind(PREFIX)
            .fetch_all(&mut *tx)
            .await?;
    for (id, secret) in rows {
        sqlx::query("UPDATE repo_push_mirrors SET secret = ? WHERE id = ?")
            .bind(key.seal(secret.as_bytes())?)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sealed += 1;
    }

    let document: Option<String> = sqlx::query_scalar("SELECT value FROM instance_settings WHERE key = ?")
        .bind(DEPLOYMENT_CONFIG_KEY)
        .fetch_optional(&mut *tx)
        .await?;
    // A document that is not JSON is left for the configuration loader to report.
    if let Some(mut value) = document.and_then(|document| serde_json::from_str::<Value>(&document).ok())
        && key.seal_secret_fields(&mut value)?
    {
        sqlx::query("UPDATE instance_settings SET value = ? WHERE key = ?")
            .bind(value.to_string())
            .bind(DEPLOYMENT_CONFIG_KEY)
            .execute(&mut *tx)
            .await?;
        sealed += 1;
    }

    tx.commit().await?;
    if sealed > 0 {
        tracing::info!(sealed, "sealed credentials that were stored in plaintext");
    }
    Ok(())
}

fn first_sealed_field(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => map.iter().find_map(|(name, value)| match value {
            Value::String(text) if is_secret_field(name) && is_sealed(text.as_bytes()) => Some(text.clone()),
            _ => first_sealed_field(value),
        }),
        Value::Array(values) => values.iter().find_map(first_sealed_field),
        _ => None,
    }
}
