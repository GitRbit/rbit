//! SQLite persistence via `sqlx`.

mod access_tokens;
mod accounts;
pub mod admin;
pub mod audit;
pub mod collaborators;
pub mod forks;
pub mod instance;
pub mod issue_meta;
pub mod issue_search;
pub mod issues;
pub mod mirrors;
pub mod notifications;
pub mod orgs;
pub mod preferences;
pub mod profiles;
pub mod protection;
pub mod pulls;
pub mod recovery;
pub mod releases;
pub mod repo_settings;
mod repositories;
pub mod reviews;
mod rows;
pub mod ssh_keys;
// Milestone 3: commit statuses (S).
pub mod stars;
pub mod statuses;
pub mod teams;
pub mod two_factor;
pub mod webhooks;

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

    // Milestone 2 wave A.

    pub fn collaborators(&self) -> collaborators::CollaboratorStore {
        collaborators::CollaboratorStore::new(self.pool.clone())
    }

    pub fn teams(&self) -> teams::TeamStore {
        teams::TeamStore::new(self.pool.clone())
    }

    pub fn webhooks(&self) -> webhooks::WebhookStore {
        webhooks::WebhookStore::new(self.pool.clone())
    }

    pub fn releases(&self) -> releases::ReleaseStore {
        releases::ReleaseStore::new(self.pool.clone())
    }

    pub fn forks(&self) -> forks::ForkStore {
        forks::ForkStore::new(self.pool.clone())
    }

    pub fn mirrors(&self) -> mirrors::MirrorStore {
        mirrors::MirrorStore::new(self.pool.clone())
    }

    // Frontend wave (T).

    pub fn preferences(&self) -> preferences::PreferenceStore {
        preferences::PreferenceStore::new(self.pool.clone())
    }

    // Milestone 2 wave B: the shared issue core (T0), then the workstreams.

    pub fn issues(&self) -> issues::IssueStore {
        issues::IssueStore::new(self.pool.clone())
    }

    pub fn issue_meta(&self) -> issue_meta::IssueMetaStore {
        issue_meta::IssueMetaStore::new(self.pool.clone())
    }

    pub fn issue_search(&self) -> issue_search::IssueSearchStore {
        issue_search::IssueSearchStore::new(self.pool.clone())
    }

    pub fn notifications(&self) -> notifications::NotificationStore {
        notifications::NotificationStore::new(self.pool.clone())
    }

    pub fn pulls(&self) -> pulls::PullStore {
        pulls::PullStore::new(self.pool.clone())
    }

    pub fn reviews(&self) -> reviews::ReviewStore {
        reviews::ReviewStore::new(self.pool.clone())
    }

    pub fn branch_protection(&self) -> protection::BranchProtectionStore {
        protection::BranchProtectionStore::new(self.pool.clone())
    }

    // Milestone 3: commit statuses (S).

    pub fn commit_statuses(&self) -> statuses::CommitStatusStore {
        statuses::CommitStatusStore::new(self.pool.clone())
    }

    /// Flushes the WAL and closes all connections. Call during graceful shutdown.
    pub async fn close(&self) {
        self.pool.close().await;
    }
}
