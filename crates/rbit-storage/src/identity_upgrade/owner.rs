//! Existing-store owner; only explicit install migrates. No key/reseal/ordinary KV initialization or public v2 pool.
use super::{
    IdentityUpgradeLimits, IdentityUpgradeSummary,
    model::{Checkpoint, IdentityUpgradeError as Error},
    sql,
};
use crate::{error::kv_error, kv::sessions::epoch::stopped as kv, sqlite::writer_compatibility};
use sqlx::{
    Connection, SqliteConnection,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
};
use std::{path::Path, sync::Arc, time::Duration};
pub struct StoppedIdentityUpgrade {
    pub(super) connection: SqliteConnection,
    pub(super) db: Arc<redb::Database>,
    pub(super) limits: IdentityUpgradeLimits,
}
/// Keeps the actual accepted SQL connection and exclusive KV handle alive. It is
/// PREPARED data/ownership, never network readiness or a session issuance receipt.
pub struct PreparedIdentityPair {
    _owner: StoppedIdentityUpgrade,
    pub(super) checkpoint: Checkpoint,
}
impl std::fmt::Debug for StoppedIdentityUpgrade {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("StoppedIdentityUpgrade").finish_non_exhaustive()
    }
}
impl std::fmt::Debug for PreparedIdentityPair {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("PreparedIdentityPair").finish_non_exhaustive()
    }
}
impl PreparedIdentityPair {
    pub fn summary(&self) -> IdentityUpgradeSummary {
        self.checkpoint.into()
    }
}
impl StoppedIdentityUpgrade {
    /// The caller must stop the deployment and retain a consistent stopped backup.
    /// An actual existing redb exclusive handle is acquired, not inferred from paths,
    /// a marker, a supplied bool, a second mutex or absence of a process-list match.
    pub async fn open(sqlite: &Path, redb: &Path, limits: IdentityUpgradeLimits) -> Result<Self, Error> {
        let mut owner = Self::open_for_install(sqlite, redb, limits).await?;
        if super::layout::history(&mut owner.connection).await? < 67 {
            return Err(Error::Invalid);
        }
        Ok(owner)
    }
    /// Opens recognized pinned-main or supported rewrite stores without installing anything.
    pub async fn open_for_install(sqlite: &Path, redb: &Path, limits: IdentityUpgradeLimits) -> Result<Self, Error> {
        if !cfg!(any(target_os = "windows", target_os = "linux", target_os = "macos")) {
            return Err(Error::Unsupported);
        }
        let path = redb.to_path_buf();
        let db = tokio::task::spawn_blocking(move || redb::Database::open(path).map_err(kv_error)).await??;
        let options = writer_compatibility::register_stopped(
            SqliteConnectOptions::new()
                .filename(sqlite)
                .create_if_missing(false)
                .foreign_keys(true)
                .pragma("trusted_schema", "OFF")
                .journal_mode(SqliteJournalMode::Wal)
                .synchronous(SqliteSynchronous::Full)
                .busy_timeout(Duration::from_secs(5)),
        );
        let mut connection = SqliteConnection::connect_with(&options).await?;
        super::layout::history(&mut connection).await?;
        Ok(Self { connection, db: Arc::new(db), limits })
    }
    /// Explicitly installs the known immutable layout suffix under the native owner.
    /// It neither clears sessions nor reseals credentials; cancellation detaches only
    /// the waiter and failures retain ownership for explicit inspection/retry.
    pub async fn install_layout(self) -> Result<Self, super::IdentityUpgradeFailure> {
        Ok(self.execute_owned(super::owned::Operation::Install).await?.owner)
    }
    /// Explicit one-time reauthentication cutover; not runtime activation.
    pub async fn prepare(self) -> Result<PreparedIdentityPair, super::IdentityUpgradeFailure> {
        let outcome = self.execute_owned(super::owned::Operation::Prepare).await?;
        Ok(PreparedIdentityPair { _owner: outcome.owner, checkpoint: outcome.checkpoint.expect("prepared operation returns checkpoint") })
    }
    pub async fn accept_prepared(self) -> Result<PreparedIdentityPair, super::IdentityUpgradeFailure> {
        let outcome = self.execute_owned(super::owned::Operation::Accept).await?;
        Ok(PreparedIdentityPair { _owner: outcome.owner, checkpoint: outcome.checkpoint.expect("accept operation returns checkpoint") })
    }
    pub(super) async fn prepare_inner(&mut self) -> Result<Checkpoint, Error> {
        let checkpoint = self.begin_preparing().await?;
        let db = Arc::clone(&self.db);
        let limits = self.limits;
        tokio::task::spawn_blocking(move || kv::prepare(&db, checkpoint, limits)).await??;
        let mut tx = self.connection.begin_with("BEGIN IMMEDIATE").await?;
        sql::within_limits(&mut tx, self.limits).await?;
        match sql::state(&mut *tx).await? {
            sql::State::Preparing(current) if current == checkpoint => sql::rebase(&mut tx, checkpoint).await?,
            sql::State::Prepared(current) if current == checkpoint => sql::verify_accounts(&mut tx, checkpoint).await?,
            _ => return Err(Error::Mismatch),
        }
        tx.commit().await?;
        self.validate_prepared().await
    }
    /// Startup acceptance for a PREPARED pair only. Ordinary legacy opens refuse it;
    /// an active protocol and all producer interlocks are intentionally not available.
    pub(super) async fn validate_prepared(&mut self) -> Result<Checkpoint, Error> {
        let mut tx = self.connection.begin_with("BEGIN IMMEDIATE").await?;
        if super::layout::history(&mut tx).await? < 67 {
            return Err(Error::Invalid);
        }
        sql::within_limits(&mut tx, self.limits).await?;
        let sql::State::Prepared(checkpoint) = sql::state(&mut *tx).await? else { return Err(Error::NotPrepared) };
        let db = Arc::clone(&self.db);
        let limits = self.limits;
        tokio::task::spawn_blocking(move || kv::verify_empty(&db, checkpoint, limits)).await??;
        sql::verify_accounts(&mut tx, checkpoint).await?;
        tx.commit().await?;
        Ok(checkpoint)
    }
    pub(super) async fn begin_preparing(&mut self) -> Result<Checkpoint, Error> {
        let mut tx = self.connection.begin_with("BEGIN IMMEDIATE").await?;
        if super::layout::history(&mut tx).await? < 67 {
            return Err(Error::Invalid);
        }
        sql::within_limits(&mut tx, self.limits).await?;
        let db = Arc::clone(&self.db);
        let limits = self.limits;
        let scan = tokio::task::spawn_blocking(move || kv::inspect(&db, limits)).await??;
        let checkpoint = match sql::state(&mut *tx).await? {
            sql::State::Legacy => {
                if scan.checkpoint.is_some() {
                    return Err(Error::Mismatch);
                }
                let history = sql::history(&mut *tx).await?;
                let pair = match (sql::pair(&mut *tx).await?, scan.pair) {
                    (Some(a), Some(b)) if a == b => a,
                    (None, None) => random()?,
                    _ => return Err(Error::Mismatch),
                };
                let checkpoint = Checkpoint {
                    generation: random()?,
                    pair,
                    floor: history.floor.max(scan.floor),
                    retired: history.retired,
                    accounts: history.accounts,
                };
                checkpoint.final_floor()?;
                sql::write_preparing(&mut tx, checkpoint).await?;
                checkpoint
            }
            sql::State::Preparing(checkpoint) => {
                let history = sql::history(&mut *tx).await?;
                if history.floor > checkpoint.floor || history.retired != checkpoint.retired || history.accounts != checkpoint.accounts {
                    return Err(Error::Mismatch);
                }
                if scan.floor > checkpoint.floor
                    || scan.pair.is_some_and(|pair| pair != checkpoint.pair)
                    || sql::pair(&mut *tx).await? != Some(checkpoint.pair)
                {
                    return Err(Error::Mismatch);
                }
                if let Some(current) = scan.checkpoint
                    && current != checkpoint
                {
                    return Err(Error::Mismatch);
                }
                checkpoint
            }
            sql::State::Prepared(checkpoint) => {
                sql::verify_accounts(&mut tx, checkpoint).await?;
                if scan.checkpoint != Some(checkpoint) || scan.pair != Some(checkpoint.pair) {
                    return Err(Error::Mismatch);
                }
                checkpoint
            }
        };
        tx.commit().await?;
        Ok(checkpoint)
    }
}
fn random() -> Result<[u8; 16], Error> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::Invalid)?;
    if bytes == [0; 16] {
        return Err(Error::Invalid);
    }
    Ok(bytes)
}
