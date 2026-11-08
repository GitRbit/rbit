//! Read-only checks of a database copy before it is put into service (`rbit restore`):
//! that it is intact, that this binary knows every migration it records, and that a key
//! file opens the credentials sealed in it. Nothing here writes to the copy.

use std::{path::Path, str::FromStr};

use sqlx::{
    ConnectOptions, Connection, SqliteConnection,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use super::{reseal, writer_compatibility};
use crate::{StorageError, sealing::SealingKey};

/// What [`inspect`] found wrong with a database copy; empty lists mean nothing.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SnapshotReport {
    /// Migrations the copy records that this binary does not embed: it was written by a
    /// newer rbit.
    pub unknown_migrations: Vec<i64>,
    /// Migrations the copy records as started but not finished.
    pub failed_migrations: Vec<i64>,
    /// `PRAGMA quick_check` messages other than `ok`.
    pub integrity: Vec<String>,
}

impl SnapshotReport {
    /// Whether the copy can be put into service by this binary.
    pub fn is_clean(&self) -> bool {
        self.unknown_migrations.is_empty() && self.failed_migrations.is_empty() && self.integrity.is_empty()
    }
}

/// The versions of the migrations embedded in this binary, ascending.
pub fn embedded_migrations() -> Vec<i64> {
    sqlx::migrate!("./migrations").iter().filter(|migration| migration.migration_type.is_up_migration()).map(|m| m.version).collect()
}

fn read_only(path: &Path) -> Result<SqliteConnectOptions, StorageError> {
    Ok(writer_compatibility::register(SqliteConnectOptions::from_str("sqlite:")?).filename(path).read_only(true).create_if_missing(false))
}

/// Checks the database at `path` without changing it.
pub async fn inspect(path: &Path) -> Result<SnapshotReport, StorageError> {
    let mut connection: SqliteConnection = read_only(path)?.connect().await?;
    let mut report = SnapshotReport::default();
    let integrity: Vec<String> = sqlx::query_scalar("PRAGMA quick_check").fetch_all(&mut connection).await?;
    report.integrity = integrity.into_iter().filter(|line| line != "ok").collect();
    let has_table: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = '_sqlx_migrations')")
            .fetch_one(&mut connection)
            .await?;
    if has_table {
        let recorded: Vec<(i64, bool)> =
            sqlx::query_as("SELECT version, success FROM _sqlx_migrations ORDER BY version").fetch_all(&mut connection).await?;
        let known = embedded_migrations();
        report.unknown_migrations = recorded.iter().filter(|(version, _)| !known.contains(version)).map(|(version, _)| *version).collect();
        report.failed_migrations = recorded.iter().filter(|(_, success)| !success).map(|(version, _)| *version).collect();
    } else {
        report.integrity.push("no migration history: this is not an rbit database".to_owned());
    }
    connection.close().await?;
    Ok(report)
}

/// Fails with [`StorageError::MissingKey`] when the database at `path` holds sealed
/// credentials and `key_file` does not exist, and with [`StorageError::WrongKey`] when the
/// key does not open them. A database with nothing sealed accepts any key, or none.
pub async fn check_key(path: &Path, key_file: &Path) -> Result<(), StorageError> {
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(read_only(path)?).await?;
    let result = async {
        match SealingKey::load(key_file)? {
            Some(key) => reseal::check_key(&pool, &key, key_file).await,
            None if reseal::sample(&pool).await?.is_some() => Err(StorageError::MissingKey(key_file.display().to_string())),
            None => Ok(()),
        }
    }
    .await;
    pool.close().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;

    #[tokio::test]
    async fn a_current_database_is_clean_and_a_newer_one_is_named() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rbit.db");
        let database = Database::open(&path).await.unwrap();
        database.server_secrets().get_or_insert("probe", b"value").await.unwrap();
        database.close().await;
        assert!(inspect(&path).await.unwrap().is_clean());
        check_key(&path, &dir.path().join("secret.key")).await.unwrap();
        assert!(matches!(check_key(&path, &dir.path().join("absent.key")).await, Err(StorageError::MissingKey(_))));
        let other = dir.path().join("other.key");
        SealingKey::create(&other).unwrap();
        assert!(matches!(check_key(&path, &other).await, Err(StorageError::WrongKey(_))));

        let mut connection = SqliteConnectOptions::new().filename(&path).connect().await.unwrap();
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (99999, 'future', 1, x'00', 0)",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        connection.close().await.unwrap();
        let report = inspect(&path).await.unwrap();
        assert_eq!(report.unknown_migrations, [99999]);
        assert!(!report.is_clean());
    }
}
