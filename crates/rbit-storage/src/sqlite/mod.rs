//! `SQLite` persistence via `sqlx`.

mod access_tokens;
mod accounts;
// Actions: foundation (F).
mod actions_rows;
// Actions: artifacts (A).
pub mod actions_artifacts;
pub mod actions_runners;
pub mod actions_runs;
pub mod actions_schedules;
pub mod actions_secrets;
pub mod actions_settings;
pub mod admin;
pub mod audit;
pub mod collaborators;
// Email digests.
pub mod email_digests;
// Milestone 3: external sign-in (O).
pub mod external_identities;
pub mod forks;
// Milestone 3: importing and push mirrors (M).
pub mod imports;
pub mod instance;
pub mod issue_meta;
pub mod issue_search;
pub mod issues;
pub mod lfs;
pub mod mirrors;
pub mod notification_email;
pub mod notifications;
pub mod orgs;
mod pool;
pub mod preferences;
pub mod profiles;
pub mod protection;
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
pub mod server_secrets;
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
pub use accounts::{AccountStore, LoginRecord, NewUser, SessionUser};
pub use repositories::{ForkParent, NewRepository, RepositoryPage, RepositoryStore};

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
        sqlx::migrate!("./migrations").run(&pool).await?;

        let key = match SealingKey::load(key_file)? {
            Some(key) => key,
            None if reseal::sample(&pool).await?.is_some() => {
                return Err(StorageError::MissingKey(key_file.display().to_string()));
            }
            None => SealingKey::create(key_file)?,
        };
        reseal::check_key(&pool, &key, key_file).await?;
        reseal::seal_plaintext(&pool, &key).await?;
        let database = Self { pool, key };
        database.optimize().await?;
        Ok(database)
    }

    /// Applies migrations embedded in the binary at compile time.
    pub async fn migrate(&self) -> Result<(), StorageError> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    /// Refreshes the query planner's table statistics where they are missing or out of
    /// date (`PRAGMA optimize`), so it can tell a selective index from a poor one. Run
    /// after migrations (new indexes) and from periodic maintenance: `SQLite` only
    /// analyses tables whose size changed a lot since their last analysis, with a bounded
    /// sample, so it usually does nothing. `0x10002` considers every table, not only those
    /// the pooled connection it happens to run on has queried.
    pub async fn optimize(&self) -> Result<(), StorageError> {
        sqlx::query("PRAGMA optimize = 0x10002").execute(&self.pool).await?;
        Ok(())
    }

    /// The shared pool, labelled with `store` for the contention metrics (`pool`).
    fn store(&self, store: &'static str) -> pool::Pool {
        pool::Pool::new(self.pool.clone(), store)
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

    pub fn access_tokens(&self) -> AccessTokenStore {
        AccessTokenStore::new(self.store("access_tokens"))
    }

    pub fn audit(&self) -> audit::AuditStore {
        audit::AuditStore::new(self.store("audit"))
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

    pub fn actions_schedules(&self) -> actions_schedules::ActionsScheduleStore {
        actions_schedules::ActionsScheduleStore::new(self.pool.clone())
    }

    // Actions: artifacts (A).
    pub fn actions_artifacts(&self) -> actions_artifacts::ActionsArtifactStore {
        actions_artifacts::ActionsArtifactStore::new(self.pool.clone())
    }

    /// Flushes the WAL and closes all connections. Call during graceful shutdown.
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

/// Query plan assertions for the stores' unit tests: the plan is the evidence that a
/// statement reads only the rows it needs.
#[cfg(test)]
pub(crate) mod plans {
    use super::Database;

    /// A freshly migrated database in a temporary directory (kept alive by the guard).
    pub(crate) async fn database() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        (dir, database)
    }

    #[tokio::test]
    async fn optimize_analyses_tables_that_gained_rows() {
        let (_dir, database) = database().await;
        let analyzed = || async {
            sqlx::query_scalar::<_, String>("SELECT tbl FROM sqlite_stat1 WHERE tbl = 'accounts'")
                .fetch_all(&database.pool)
                .await
                .unwrap_or_default()
        };
        assert!(analyzed().await.is_empty(), "empty tables are left alone");
        sqlx::query("INSERT INTO accounts (kind, name, created_at, updated_at) VALUES ('user', 'ada', 0, 0)")
            .execute(&database.pool)
            .await
            .unwrap();
        database.optimize().await.unwrap();
        assert!(!analyzed().await.is_empty(), "the planner now knows the table's size");
    }

    /// `.cargo/config.toml` builds the bundled `SQLite` with the query planner stability
    /// guarantee on, so no statement is re-prepared because a parameter was rebound.
    #[tokio::test]
    async fn plans_never_depend_on_bound_values() {
        let (_dir, database) = database().await;
        let qpsg: bool = sqlx::query_scalar("SELECT sqlite_compileoption_used('ENABLE_QPSG')").fetch_one(&database.pool).await.unwrap();
        assert!(qpsg, "SQLite was built without SQLITE_ENABLE_QPSG (LIBSQLITE3_FLAGS in .cargo/config.toml)");
    }

    /// The `detail` lines of `EXPLAIN QUERY PLAN sql`, in order. Parameters stay unbound.
    pub(crate) async fn query_plan(database: &Database, sql: &str) -> Vec<String> {
        let rows: Vec<(i64, i64, i64, String)> =
            sqlx::query_as(sqlx::AssertSqlSafe(format!("EXPLAIN QUERY PLAN {sql}"))).fetch_all(&database.pool).await.unwrap();
        rows.into_iter().map(|(_, _, _, detail)| detail).collect()
    }

    /// Planner statistics an instance could have: what `PRAGMA optimize` records.
    #[derive(Debug, Clone, Copy)]
    pub(crate) enum Stats {
        /// None at all, as on a new instance before its tables are analysed.
        None,
        /// Every table analysed at one row, as on a new instance with its first account
        /// and repository (and as long as no table has grown enough to be re-analysed).
        Tiny,
        /// Every table analysed at a million rows, ten per value of an index's first
        /// column and one per unique key.
        Large,
    }

    pub(crate) const STATS: [Stats; 3] = [Stats::None, Stats::Tiny, Stats::Large];

    /// The `detail` lines of `EXPLAIN QUERY PLAN sql` with the planner believing `stats`:
    /// on one connection of its own, with `sqlite_stat1` written for every table and
    /// index (and `sqlite_stat4` emptied), then reloaded with `ANALYZE sqlite_schema`.
    pub(crate) async fn query_plan_with(database: &Database, stats: Stats, sql: &str) -> Vec<String> {
        let mut conn = database.pool.acquire().await.unwrap();
        // Creates the statistics tables if this database has none yet.
        sqlx::query("ANALYZE sqlite_schema").execute(&mut *conn).await.unwrap();
        sqlx::query("DELETE FROM sqlite_stat1").execute(&mut *conn).await.unwrap();
        let has_stat4: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE name = 'sqlite_stat4')")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        if has_stat4 {
            sqlx::query("DELETE FROM sqlite_stat4").execute(&mut *conn).await.unwrap();
        }
        let rows = match stats {
            Stats::None => None,
            Stats::Tiny => Some(1_u64),
            Stats::Large => Some(1_000_000),
        };
        if let Some(rows) = rows {
            let tables: Vec<String> = sqlx::query_scalar(
                "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND sql NOT LIKE 'CREATE VIRTUAL%'",
            )
            .fetch_all(&mut *conn)
            .await
            .unwrap();
            for table in tables {
                let indexes: Vec<(String, bool)> =
                    sqlx::query_as("SELECT name, \"unique\" FROM pragma_index_list(?)").bind(&table).fetch_all(&mut *conn).await.unwrap();
                // A table's own row count is recorded only when it has no index.
                let mut entries = if indexes.is_empty() { vec![(None, rows.to_string())] } else { Vec::new() };
                for (index, unique) in indexes {
                    let columns: i64 =
                        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_index_info(?)").bind(&index).fetch_one(&mut *conn).await.unwrap();
                    let per_column = (1..=columns).map(|column| {
                        if unique && column == columns { 1 } else { (rows / 10_u64.pow(u32::try_from(column).unwrap())).max(1) }
                    });
                    let stat = std::iter::once(rows).chain(per_column).map(|n| n.to_string()).collect::<Vec<_>>().join(" ");
                    entries.push((Some(index), stat));
                }
                for (index, stat) in entries {
                    sqlx::query("INSERT INTO sqlite_stat1 (tbl, idx, stat) VALUES (?, ?, ?)")
                        .bind(&table)
                        .bind(index)
                        .bind(stat)
                        .execute(&mut *conn)
                        .await
                        .unwrap();
                }
            }
        }
        sqlx::query("ANALYZE sqlite_schema").execute(&mut *conn).await.unwrap();
        let rows: Vec<(i64, i64, i64, String)> =
            sqlx::query_as(sqlx::AssertSqlSafe(format!("EXPLAIN QUERY PLAN {sql}"))).fetch_all(&mut *conn).await.unwrap();
        // Leave no statistics behind for the next test on this pool.
        sqlx::query("DELETE FROM sqlite_stat1").execute(&mut *conn).await.unwrap();
        sqlx::query("ANALYZE sqlite_schema").execute(&mut *conn).await.unwrap();
        rows.into_iter().map(|(_, _, _, detail)| detail).collect()
    }

    /// Fails unless `sql` reads every table by a key under every [`STATS`]: no plan line
    /// is a `SCAN` other than those starting with one of `allowed` (such as a JSON list of
    /// ids, `SCAN json_each`).
    pub(crate) async fn assert_no_scans(database: &Database, sql: &str, allowed: &[&str]) {
        for stats in STATS {
            let plan = query_plan_with(database, stats, sql).await;
            let scans: Vec<_> =
                plan.iter().filter(|detail| detail.starts_with("SCAN") && !allowed.iter().any(|ok| detail.starts_with(ok))).collect();
            assert!(scans.is_empty(), "{stats:?} statistics: {scans:?} in {plan:#?}");
        }
    }

    #[tokio::test]
    async fn statistics_scenarios_reach_the_planner() {
        let (_dir, database) = database().await;
        // With one-row statistics a join by primary key is cheaper as a scan of the
        // inner table, so the scenarios do change plans.
        let sql = "SELECT f.repo_id FROM repo_forks f JOIN repositories r ON r.id = f.parent_id WHERE f.created_at = 0";
        let tiny = query_plan_with(&database, Stats::Tiny, sql).await;
        let large = query_plan_with(&database, Stats::Large, sql).await;
        assert_ne!(tiny, large, "{tiny:#?}");
    }
}
