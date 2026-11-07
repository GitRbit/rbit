//! Exact immutable migration-prefix validation and explicit stopped installation.
#[path = "layout_queries.rs"]
mod queries;
use super::{model::IdentityUpgradeError as Error, owner::StoppedIdentityUpgrade, sql};
use crate::kv::sessions::epoch::stopped as kv;
use sqlx::{Connection, SqliteConnection};
use std::sync::Arc;
const MAIN_VERSION: i64 = 59;
#[derive(sqlx::FromRow)]
struct MigrationRow {
    version: i64,
    success: i64,
    checksum: Option<Vec<u8>>,
}
pub(super) async fn history(connection: &mut SqliteConnection) -> Result<i64, Error> {
    let migrator = sqlx::migrate!("./migrations");
    let limit = i64::try_from(migrator.migrations.len() + 1).map_err(|_| Error::Invalid)?;
    let shape: i64 = sqlx::query_scalar(queries::CORE_SHAPE).fetch_one(&mut *connection).await?;
    if shape != 3 {
        return Err(Error::Invalid);
    }
    let keys: bool = sqlx::query_scalar(queries::CORE_KEYS).fetch_one(&mut *connection).await?;
    if !keys {
        return Err(Error::Invalid);
    }
    let rows: Vec<MigrationRow> = sqlx::query_as(queries::MIGRATIONS).bind(limit).fetch_all(&mut *connection).await?;
    if rows.len() > migrator.migrations.len() || rows.last().is_none_or(|row| row.version < MAIN_VERSION) {
        return Err(Error::Invalid);
    }
    for (row, expected) in rows.iter().zip(migrator.migrations.iter()) {
        if row.version != expected.version || row.success != 1 || row.checksum.as_deref() != Some(expected.checksum.as_ref()) {
            return Err(Error::Invalid);
        }
    }
    let version = rows.last().ok_or(Error::Invalid)?.version;
    if version >= 67 {
        let guards: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE type='trigger' AND name GLOB 'identity_v2_*'")
            .fetch_one(&mut *connection)
            .await?;
        let control:i64=sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE type='trigger' AND name IN ('credential_protocol_insert','credential_protocol_update','credential_protocol_retained')").fetch_one(&mut *connection).await?;
        if guards != 45 || control != 3 {
            return Err(Error::Invalid);
        }
    }
    Ok(version)
}
pub(super) async fn legacy_mode(connection: &mut SqliteConnection) -> Result<(), Error> {
    let shape: Vec<i64> = sqlx::query_scalar(queries::PROTOCOL_SHAPE).fetch_all(&mut *connection).await?;
    match shape.as_slice() {
        [] => {
            let metadata: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='_sqlx_migrations' AND type='table')")
                    .fetch_one(&mut *connection)
                    .await?;
            if metadata {
                let installed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version>=67)")
                    .fetch_one(&mut *connection)
                    .await?;
                if installed {
                    return Err(Error::Invalid);
                }
            }
            Ok(())
        }
        [1] => match sql::state(&mut *connection).await? {
            sql::State::Legacy => Ok(()),
            _ => Err(Error::LegacyModeRequired),
        },
        _ => Err(Error::Invalid),
    }
}
impl StoppedIdentityUpgrade {
    pub(super) async fn install_inner(&mut self) -> Result<(), Error> {
        // Freeze SQLite-only official writers as well as retaining the native KV owner.
        // The migrator uses nested savepoints; the outer commit installs the whole suffix.
        let mut tx = self.connection.begin_with("BEGIN IMMEDIATE").await?;
        let installed = history(&mut tx).await?;
        legacy_mode(&mut tx).await?;
        let db = Arc::clone(&self.db);
        let limits = self.limits;
        let scan = tokio::task::spawn_blocking(move || kv::inspect(&db, limits)).await??;
        if scan.checkpoint.is_some() {
            return Err(Error::Mismatch);
        }
        let migrator = sqlx::migrate!("./migrations");
        // Already-applied no-transaction migrations (including immutable 0055)
        // are checksum-validated history; run_direct skips them. Only a pending
        // no-transaction suffix would violate this operation's outer transaction.
        if migrator.migrations.iter().any(|migration| migration.version > installed && migration.no_tx) {
            return Err(Error::Unsupported);
        }
        // SQLx's direct API preserves this exact connection/savepoint scope and
        // avoids its Acquire HRTB limitation when the owning task must be Send.
        migrator.run_direct(None, &mut *tx, false).await.map_err(crate::StorageError::from)?;
        if history(&mut tx).await? < 67 {
            return Err(Error::Invalid);
        }
        legacy_mode(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
