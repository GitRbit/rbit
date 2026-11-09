//! The database side of `rbit backup` and `rbit restore`: the consistent snapshot, what it
//! says a backup must hold, which rows leave it when their files are gone, and the wiki
//! catalog a backup format can or cannot restore.
//!
//! The server composes these with its file and Git copies; every SQL statement a backup or
//! restore runs lives here, so the decisions about the snapshot cannot drift from the
//! schema they read. Invariants:
//!
//! * [`snapshot_into`] opens the live database without migrating it: backing up never
//!   changes the source, also when it predates this binary's migrations.
//! * [`inventory::read`] lists exactly the stored files the backup [`FORMAT_VERSION`] (or
//!   an older format being restored) holds, at their data-directory-relative paths.
//! * [`prune::prune`] removes, in one transaction, every row whose repository or file the
//!   backup could not copy, so a restored database never refers to something missing.
//! * Restore checks (digest, `quick_check`, unknown migrations and the sealing key) stay in
//!   [`super::snapshot`] and never write to the copy.

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use sqlx::{ConnectOptions, Connection, SqliteConnection, sqlite::SqliteConnectOptions};

use super::writer_compatibility;

pub mod inventory;
pub mod prune;
pub mod wikis;

/// Bumped whenever the layout of a backup directory changes (2: release attachments,
/// 3: Git LFS objects, 4: the secret key file, 5: Actions artifacts, caches and logs,
/// 6: repository wiki objects).
pub const FORMAT_VERSION: u32 = 6;

/// What reading or changing a snapshot fails with. Messages match what `rbit backup` and
/// `rbit restore` have always reported; database errors pass through unchanged.
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error(transparent)]
    Sqlite(#[from] sqlx::Error),
    #[error("cannot open {}", .path.display())]
    Open {
        path: PathBuf,
        #[source]
        source: sqlx::Error,
    },
    #[error("the backup path must be valid UTF-8")]
    PathNotUtf8,
    #[error("VACUUM INTO failed")]
    Vacuum(#[source] sqlx::Error),
    #[error("invalid LFS object id {0:?}")]
    LfsOid(String, #[source] rbit_core::lfs::InvalidOid),
    #[error(transparent)]
    Digest(#[from] std::num::ParseIntError),
    #[error("invalid wiki store identity")]
    WikiIdentity,
    #[error("invalid wiki revision {0:?}")]
    WikiRevision(String),
    #[error("invalid wiki id {0}")]
    WikiId(i64),
    #[error("cannot leave repository {0}, deleted during the backup, out of it; take the backup again")]
    PruneRepository(i64, #[source] sqlx::Error),
}

/// `VACUUM INTO`: a consistent, compacted copy of the live database at `source`, written
/// into the existing (empty) file `snapshot` in one read transaction, so writers are never
/// blocked. Opened without running migrations, so backing up never changes the source.
pub async fn snapshot_into(source: &Path, snapshot: &Path) -> Result<(), BackupError> {
    let target = snapshot.to_str().ok_or(BackupError::PathNotUtf8)?;
    let mut connection = writer_compatibility::register(SqliteConnectOptions::from_str("sqlite:")?)
        .filename(source)
        .create_if_missing(false)
        .busy_timeout(std::time::Duration::from_secs(30))
        .connect()
        .await
        .map_err(|source_error| BackupError::Open { path: source.to_owned(), source: source_error })?;
    sqlx::query("VACUUM INTO ?").bind(target).execute(&mut connection).await.map_err(BackupError::Vacuum)?;
    connection.close().await?;
    Ok(())
}

/// How many Actions secrets the database at `database` seals with the separate Actions
/// key, which no backup holds.
pub async fn sealed_actions_secrets(database: &Path) -> Result<i64, BackupError> {
    let mut connection = read_only(database).await?;
    let secrets: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actions_secrets").fetch_one(&mut connection).await?;
    connection.close().await?;
    Ok(secrets)
}

/// A read-only connection to the snapshot (or restored copy) at `snapshot`.
async fn read_only(snapshot: &Path) -> Result<SqliteConnection, BackupError> {
    let options = writer_compatibility::register(SqliteConnectOptions::from_str("sqlite:")?);
    options.filename(snapshot).read_only(true).connect().await.map_err(|source| BackupError::Open { path: snapshot.to_owned(), source })
}

/// A writable connection with foreign keys on, so deleting a row cascades as on the live
/// database.
async fn writable(snapshot: &Path) -> Result<SqliteConnection, BackupError> {
    let options = writer_compatibility::register(SqliteConnectOptions::new()).filename(snapshot).foreign_keys(true);
    options.connect().await.map_err(|source| BackupError::Open { path: snapshot.to_owned(), source })
}
