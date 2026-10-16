//! SQLite persistence via `sqlx`.

mod access_tokens;
mod accounts;
pub mod admin;
pub mod audit;
pub mod instance;
pub mod orgs;
pub mod profiles;
pub mod recovery;
pub mod repo_settings;
mod repositories;
mod rows;
pub mod ssh_keys;
pub mod stars;
pub mod two_factor;

use std::{path::Path, str::FromStr, time::Duration};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};

pub use access_tokens::{AccessTokenStore, NewAccessToken};
pub use accounts::{AccountStore, LoginRecord, NewUser};
pub use repositories::{NewRepository, RepositoryStore};

use crate::StorageError;

/// A pooled SQLite database tuned for a write-light, read-heavy web workload.
#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Opens (creating if necessary) the database file and applies pending migrations.
    pub async fn open(path: &Path) -> Result<Self, StorageError> {
        let options = SqliteConnectOptions::from_str("sqlite:")?
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5))
            .pragma("mmap_size", "268435456")
            .pragma("temp_store", "memory");

        let pool = SqlitePoolOptions::new()
            .max_connections(std::thread::available_parallelism().map_or(4, |n| n.get().clamp(2, 16)).try_into().unwrap_or(4))
            .connect_with(options)
            .await?;

        let database = Self { pool };
        database.migrate().await?;
        Ok(database)
    }

    /// Applies migrations embedded in the binary at compile time.
    pub async fn migrate(&self) -> Result<(), StorageError> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    pub fn accounts(&self) -> AccountStore {
        AccountStore::new(self.pool.clone())
    }

    pub fn repositories(&self) -> RepositoryStore {
        RepositoryStore::new(self.pool.clone())
    }

    pub fn access_tokens(&self) -> AccessTokenStore {
        AccessTokenStore::new(self.pool.clone())
    }

    pub fn audit(&self) -> audit::AuditStore {
        audit::AuditStore::new(self.pool.clone())
    }

    pub fn recovery(&self) -> recovery::RecoveryStore {
        recovery::RecoveryStore::new(self.pool.clone())
    }

    pub fn ssh_keys(&self) -> ssh_keys::SshKeyStore {
        ssh_keys::SshKeyStore::new(self.pool.clone())
    }

    pub fn two_factor(&self) -> two_factor::TwoFactorStore {
        two_factor::TwoFactorStore::new(self.pool.clone())
    }

    pub fn admin(&self) -> admin::AdminStore {
        admin::AdminStore::new(self.pool.clone())
    }

    pub fn instance_settings(&self) -> instance::InstanceSettingsStore {
        instance::InstanceSettingsStore::new(self.pool.clone())
    }

    pub fn orgs(&self) -> orgs::OrgStore {
        orgs::OrgStore::new(self.pool.clone())
    }

    pub fn profiles(&self) -> profiles::ProfileStore {
        profiles::ProfileStore::new(self.pool.clone())
    }

    pub fn repo_settings(&self) -> repo_settings::RepoSettingsStore {
        repo_settings::RepoSettingsStore::new(self.pool.clone())
    }

    pub fn stars(&self) -> stars::StarStore {
        stars::StarStore::new(self.pool.clone())
    }

    /// Flushes the WAL and closes all connections. Call during graceful shutdown.
    pub async fn close(&self) {
        self.pool.close().await;
    }
}
