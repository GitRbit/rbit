//! `SQLite` persistence via `sqlx`.
mod access_tokens;
mod accounts;
// Actions: foundation (F).
mod actions_rows;
// Actions: artifacts (A).
pub mod actions_artifacts;
// Actions: shared cache.
pub mod actions_caches;
pub mod actions_environments;
mod actions_run_filter;
pub mod actions_runners;
pub mod actions_runs;
pub mod actions_schedules;
pub mod actions_secrets;
pub mod actions_settings;
pub mod admin;
pub mod audit;
// `rbit backup` and `rbit restore`: snapshot, inventory, pruning and the wiki catalog.
pub mod backup;
pub mod collaborators;
pub mod credential_epochs;
mod credential_mutations;
pub mod credential_revocations;
// Email digests.
pub mod email_digests;
// Milestone 3: external sign-in (O).
pub mod external_identities;
pub mod forks;
// Milestone 3: importing and push mirrors (M).
pub mod imports;
pub mod instance;
mod issue_listing_ports;
pub mod issue_meta;
pub mod issue_search;
pub mod issues;
pub mod lfs;
pub mod mirrors;
pub mod notification_email;
pub mod notifications;
mod opening;
pub mod orgs;
mod pool;
pub mod preferences;
pub mod profiles;
pub mod protection;
#[cfg(test)]
pub(crate) mod protocol;
pub mod pulls;
// Milestone 3: importing and push mirrors (M).
pub mod push_mirrors;
pub mod recovery;
pub mod releases;
pub mod repo_settings;
mod repositories;
mod reseal;
pub mod reviews;
mod rows;
mod schema;
pub mod server_secrets;
pub mod snapshot;
pub mod ssh_keys;
// Milestone 3: commit statuses (S).
pub mod stars;
pub mod statuses;
pub mod teams;
pub mod two_factor;
pub mod webhooks;
pub mod wiki_mutations;
pub mod wikis;
pub mod writer_compatibility;

use std::path::Path;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

pub use self::access_tokens::{AccessTokenStore, NewAccessToken};
pub use self::accounts::{AccountStore, LoginRecord, NewUser, SessionUser};
pub use self::repositories::{ForkParent, NewRepository, RepositoryPage, RepositoryStore};

use crate::{
    StorageError,
    sealing::{KEY_FILE_NAME, SealingKey},
};

/// A pooled `SQLite` database tuned for a write-light, read-heavy web workload.
#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
    /// Seals the credentials stores keep (see [`crate::sealing`]).
    key: SealingKey,
    #[cfg(test)]
    protocol: protocol::SqlProtocol,
}

impl Database {
    /// Opens (creating if necessary) the database file and applies pending migrations.
    /// Secrets are sealed with the key file next to it ([`KEY_FILE_NAME`]).
    pub async fn open(path: &Path) -> Result<Self, StorageError> {
        Self::open_with_key_file(path, &path.with_file_name(KEY_FILE_NAME)).await
    }

    /// [`Self::open`] with the secret key file at `key_file`. A missing key file is
    /// created, unless the database already holds values sealed with a key: then the key
    /// was lost or misplaced, and a new one would make them unreadable. Secrets stored in
    /// plaintext (before sealing existed) are sealed before this returns.
    pub async fn open_with_key_file(path: &Path, key_file: &Path) -> Result<Self, StorageError> {
        let options = writer_compatibility::database_options(path)?;

        let pool = SqlitePoolOptions::new().max_connections(connection_capacity()).connect_with(options).await?;
        crate::identity_upgrade::require_legacy_sql(&pool).await?;
        schema::refuse_newer(&pool).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        crate::identity_upgrade::require_legacy_sql(&pool).await?;

        let key = opening::key(&pool, key_file).await?;
        let database = Self {
            pool,
            key,
            #[cfg(test)]
            protocol: protocol::SqlProtocol::legacy(),
        };
        database.optimize().await?;
        Ok(database)
    }

    /// The shared pool, labelled with `store` for the contention metrics (`pool`).
    fn store(&self, store: &'static str) -> pool::Pool {
        let pool = pool::Pool::new(self.pool.clone(), store);
        #[cfg(test)]
        let pool = pool.with_protocol(self.protocol);
        pool
    }

    /// The key that seals stored credentials, for values kept outside a dedicated store
    /// (the secret fields of the saved configuration).
    pub fn sealing_key(&self) -> &SealingKey {
        &self.key
    }

    pub fn accounts(&self) -> AccountStore {
        AccountStore::new(self.store("accounts"))
    }

    pub fn repositories(&self) -> RepositoryStore {
        RepositoryStore::new(self.store("repositories"))
    }

    pub fn wikis(&self) -> wikis::WikiCatalogStore {
        wikis::WikiCatalogStore::new(self.store("wikis"))
    }

    pub fn access_tokens(&self) -> AccessTokenStore {
        AccessTokenStore::new(self.store("access_tokens"))
    }

    pub fn audit(&self) -> audit::AuditStore {
        audit::AuditStore::new(self.store("audit"))
    }

    pub fn credential_revocations(&self) -> credential_revocations::CredentialRevocationStore {
        credential_revocations::CredentialRevocationStore::new(self.store("credential_revocations"))
    }

    pub fn recovery(&self) -> recovery::RecoveryStore {
        recovery::RecoveryStore::new(self.store("recovery"))
    }

    pub fn ssh_keys(&self) -> ssh_keys::SshKeyStore {
        ssh_keys::SshKeyStore::new(self.store("ssh_keys"))
    }

    pub fn two_factor(&self) -> two_factor::TwoFactorStore {
        two_factor::TwoFactorStore::new(self.store("two_factor"), self.key.clone())
    }

    pub fn admin(&self) -> admin::AdminStore {
        admin::AdminStore::new(self.store("admin"))
    }

    pub fn instance_settings(&self) -> instance::InstanceSettingsStore {
        instance::InstanceSettingsStore::new(self.store("instance_settings"))
    }

    pub fn orgs(&self) -> orgs::OrgStore {
        orgs::OrgStore::new(self.store("orgs"))
    }

    pub fn profiles(&self) -> profiles::ProfileStore {
        profiles::ProfileStore::new(self.store("profiles"))
    }

    pub fn repo_settings(&self) -> repo_settings::RepoSettingsStore {
        repo_settings::RepoSettingsStore::new(self.store("repo_settings"))
    }

    pub fn stars(&self) -> stars::StarStore {
        stars::StarStore::new(self.store("stars"))
    }

    // Milestone 2 wave A.

    pub fn collaborators(&self) -> collaborators::CollaboratorStore {
        collaborators::CollaboratorStore::new(self.store("collaborators"))
    }

    pub fn teams(&self) -> teams::TeamStore {
        teams::TeamStore::new(self.store("teams"))
    }

    pub fn webhooks(&self) -> webhooks::WebhookStore {
        webhooks::WebhookStore::new(self.store("webhooks"), self.key.clone())
    }

    pub fn releases(&self) -> releases::ReleaseStore {
        releases::ReleaseStore::new(self.store("releases"))
    }

    pub fn forks(&self) -> forks::ForkStore {
        forks::ForkStore::new(self.store("forks"))
    }

    pub fn mirrors(&self) -> mirrors::MirrorStore {
        mirrors::MirrorStore::new(self.store("mirrors"))
    }

    // Frontend wave (T).

    pub fn preferences(&self) -> preferences::PreferenceStore {
        preferences::PreferenceStore::new(self.store("preferences"))
    }

    // Milestone 2 wave B: the shared issue core (T0), then the workstreams.

    pub fn issues(&self) -> issues::IssueStore {
        issues::IssueStore::new(self.store("issues"))
    }

    pub fn issue_meta(&self) -> issue_meta::IssueMetaStore {
        issue_meta::IssueMetaStore::new(self.store("issue_meta"))
    }

    pub fn issue_search(&self) -> issue_search::IssueSearchStore {
        issue_search::IssueSearchStore::new(self.store("issue_search"))
    }

    pub fn notifications(&self) -> notifications::NotificationStore {
        notifications::NotificationStore::new(self.store("notifications"))
    }

    pub fn pulls(&self) -> pulls::PullStore {
        pulls::PullStore::new(self.store("pulls"))
    }

    pub fn reviews(&self) -> reviews::ReviewStore {
        reviews::ReviewStore::new(self.store("reviews"))
    }

    pub fn branch_protection(&self) -> protection::BranchProtectionStore {
        protection::BranchProtectionStore::new(self.store("branch_protection"))
    }

    // Milestone 3: commit statuses (S).

    pub fn commit_statuses(&self) -> statuses::CommitStatusStore {
        statuses::CommitStatusStore::new(self.store("commit_statuses"))
    }

    // Milestone 3: email notifications (N).

    pub fn notification_email(&self) -> notification_email::NotificationEmailStore {
        notification_email::NotificationEmailStore::new(self.store("notification_email"))
    }

    // Email digests.

    pub fn email_digests(&self) -> email_digests::EmailDigestStore {
        email_digests::EmailDigestStore::new(self.store("email_digests"))
    }

    pub fn server_secrets(&self) -> server_secrets::ServerSecretStore {
        server_secrets::ServerSecretStore::new(self.store("server_secrets"), self.key.clone())
    }

    // Milestone 3: Git LFS (L).

    pub fn lfs(&self) -> lfs::LfsStore {
        lfs::LfsStore::new(self.store("lfs"))
    }

    // Milestone 3: external sign-in (O).

    pub fn external_identities(&self) -> external_identities::ExternalIdentityStore {
        external_identities::ExternalIdentityStore::new(self.store("external_identities"))
    }

    // Milestone 3: importing and push mirrors (M).

    pub fn imports(&self) -> imports::ImportStore {
        imports::ImportStore::new(self.store("imports"))
    }

    pub fn push_mirrors(&self) -> push_mirrors::PushMirrorStore {
        push_mirrors::PushMirrorStore::new(self.store("push_mirrors"), self.key.clone())
    }

    // Actions: foundation (F).

    pub fn actions_settings(&self) -> actions_settings::ActionsSettingsStore {
        actions_settings::ActionsSettingsStore::new(self.pool.clone())
    }

    pub fn actions_runners(&self) -> actions_runners::ActionsRunnerStore {
        actions_runners::ActionsRunnerStore::new(self.pool.clone())
    }

    pub fn actions_runs(&self) -> actions_runs::ActionsRunStore {
        actions_runs::ActionsRunStore::new(self.pool.clone())
    }

    pub fn actions_secrets(&self) -> actions_secrets::ActionsSecretStore {
        actions_secrets::ActionsSecretStore::new(self.pool.clone())
    }

    // Actions: environments.
    pub fn actions_environments(&self) -> actions_environments::ActionsEnvironmentStore {
        actions_environments::ActionsEnvironmentStore::new(self.pool.clone())
    }

    pub fn actions_schedules(&self) -> actions_schedules::ActionsScheduleStore {
        actions_schedules::ActionsScheduleStore::new(self.pool.clone())
    }

    // Actions: artifacts (A).
    pub fn actions_artifacts(&self) -> actions_artifacts::ActionsArtifactStore {
        actions_artifacts::ActionsArtifactStore::new(self.pool.clone())
    }

    // Actions: shared cache.
    pub fn actions_caches(&self) -> actions_caches::ActionsCacheStore {
        actions_caches::ActionsCacheStore::new(self.pool.clone())
    }

    /// Flushes the WAL and closes all connections. Call during graceful shutdown.
    pub async fn close(&self) {
        self.pool.close().await;
    }
}
pub(crate) fn connection_capacity() -> u32 {
    std::thread::available_parallelism().map_or(4, |n| n.get().clamp(2, 16)).try_into().unwrap_or(4)
}

/// Query plan assertions for the stores' unit tests: the plan is the evidence that a
/// statement reads only the rows it needs.
#[cfg(test)]
pub(crate) mod plans;
