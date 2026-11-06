//! Explicit, offline, immutable SQLite inventory. No application database initialization.
use futures_util::TryStreamExt;
use rbit_identity::inventory::{Inventory, Limits, Report};
use sqlx::{Connection, Row, SqliteConnection, sqlite::SqliteConnectOptions};
use std::path::Path;
use zeroize::Zeroizing;
mod snapshot;

/// Fixed errors deliberately cannot carry paths, SQL text, credentials or driver messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanError {
    Input,
    Sidecar,
    Drift,
    Database,
    Schema,
    Limits,
}
impl ScanError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Input => "SNAPSHOT_INPUT_REFUSED",
            Self::Sidecar => "SNAPSHOT_SIDECAR_REFUSED",
            Self::Drift => "SNAPSHOT_DRIFT_DETECTED",
            Self::Database => "SNAPSHOT_DATABASE_REFUSED",
            Self::Schema => "SNAPSHOT_SCHEMA_REFUSED",
            Self::Limits => "INVENTORY_LIMITS_REFUSED",
        }
    }
}
/// Caller must provide an offline, trusted copy kept unchanged for this entire call.
/// Immutable mode disables SQLite locking/change detection; these external checks do
/// not make a live or maliciously concurrently replaced database safe/consistent.
pub async fn scan_snapshot(path: &Path) -> Result<Report, ScanError> {
    scan_with_hook(path, Limits::default(), |_| {}).await
}
async fn scan_with_hook(path: &Path, limits: Limits, after_query: impl FnOnce(&Path)) -> Result<Report, ScanError> {
    if !limits.valid() {
        return Err(ScanError::Limits);
    }
    let checked = snapshot::checked_path(path)?;
    let before = snapshot::fingerprint(&checked)?;
    let options = SqliteConnectOptions::new()
        .filename(&checked)
        .read_only(true)
        .immutable(true)
        .create_if_missing(false)
        .statement_cache_capacity(0)
        .row_buffer_size(1)
        .command_buffer_size(1)
        .pragma("query_only", "ON")
        .pragma("trusted_schema", "OFF")
        .pragma("cache_size", "-2048")
        .pragma("temp_store", "MEMORY")
        .pragma("mmap_size", "0");
    let mut connection = SqliteConnection::connect_with(&options).await.map_err(|_| ScanError::Database)?;
    let result = scan_rows(&mut connection, limits).await;
    let closed = connection.close().await.map_err(|_| ScanError::Database);
    after_query(&checked);
    // Validate even failed SQL scans; never publish partial output after detected drift.
    if snapshot::checked_path(path)? != checked || snapshot::fingerprint(&checked)? != before {
        return Err(ScanError::Drift);
    }
    closed?;
    result
}
async fn scan_rows(connection: &mut SqliteConnection, limits: Limits) -> Result<Report, ScanError> {
    let real_table: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE type='table' AND name='user_logins' AND rootpage>0")
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| ScanError::Schema)?;
    if real_table != 1 {
        return Err(ScanError::Schema);
    }
    let mut inventory = Inventory::new(limits).ok_or(ScanError::Limits)?;
    // octet_length(column) reads the byte length from record metadata, not the entire
    // oversized TEXT. CASE never loads that value for oversized/non-TEXT rows.
    // No identity columns, salts or digests are independently selected or retained.
    let mut rows = sqlx::query("SELECT typeof(password_hash)='text', octet_length(password_hash), CASE WHEN typeof(password_hash)='text' AND octet_length(password_hash)<=? THEN CAST(password_hash AS BLOB) ELSE X'' END FROM user_logins LIMIT ?")
        .bind(i64::from(limits.prefix_bytes)).bind(i64::from(limits.rows) + 1).fetch(connection);
    while let Some(row) = rows.try_next().await.map_err(|_| ScanError::Database)? {
        let text: bool = row.try_get(0).map_err(|_| ScanError::Schema)?;
        let length: Option<i64> = row.try_get(1).map_err(|_| ScanError::Schema)?;
        let prefix = Zeroizing::new(row.try_get::<Vec<u8>, _>(2).map_err(|_| ScanError::Schema)?);
        if prefix.len() > usize::from(limits.prefix_bytes) {
            return Err(ScanError::Limits);
        }
        let length = if text { length.and_then(|n| u64::try_from(n).ok()) } else { None };
        if !inventory.observe(&prefix, length) {
            break;
        }
    }
    Ok(inventory.finish())
}
#[cfg(test)]
mod tests;
