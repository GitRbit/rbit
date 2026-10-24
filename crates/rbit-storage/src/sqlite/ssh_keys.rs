//! User SSH keys and repository deploy keys.
//!
//! Keys arrive here already parsed and validated by the service layer; this store only
//! persists them and resolves a fingerprint (the identity a client proves at SSH sign-in)
//! to whoever holds the key. A fingerprint is unique across *both* tables, which the
//! schema enforces with triggers, so a lookup can never be ambiguous.
//!
//! Milestone 1 workstream H owns this file. Keep row types here rather than in `rows.rs`.

use rbit_core::{AccountId, RepoId, Timestamp, User, repository::OwnedRepository};
use sqlx::SqlitePool;

use super::rows::{OwnedRepositoryRow, UserRow};
use crate::StorageError;

/// `last_used_at` is rewritten at most this often, keeping Git traffic write-light.
const LAST_USED_RESOLUTION_SECONDS: i64 = 60 * 60;

/// Names the conflicting field in [`StorageError::Conflict`].
const KEY_FIELD: &str = "this key";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SshKeyId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeployKeyId(pub i64);

/// A validated public key ready to be stored.
#[derive(Debug, Clone)]
pub struct NewPublicKey {
    pub title: String,
    /// OpenSSH form without comment, e.g. `ssh-ed25519 AAAA…`.
    pub public_key: String,
    /// The SSH algorithm name, e.g. `ssh-ed25519`.
    pub algorithm: String,
    /// `SHA256:…`.
    pub fingerprint: String,
}

/// A user's SSH key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshKey {
    pub id: SshKeyId,
    pub account_id: AccountId,
    pub title: String,
    pub public_key: String,
    pub algorithm: String,
    pub fingerprint: String,
    pub created_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
}

/// A key granting access to one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployKey {
    pub id: DeployKeyId,
    pub repo_id: RepoId,
    pub title: String,
    pub public_key: String,
    pub algorithm: String,
    pub fingerprint: String,
    /// `false` when the key may also push.
    pub read_only: bool,
    pub created_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
}

/// Who holds a key, as resolved from its fingerprint.
#[derive(Debug, Clone)]
pub enum KeyHolder {
    /// A user's personal key. `suspended` must be checked before the key is honoured.
    User { key: SshKey, user: User, suspended: bool },
    /// A repository's deploy key.
    Deploy { key: DeployKey, repo: OwnedRepository },
}

#[derive(Debug, sqlx::FromRow)]
struct SshKeyRow {
    id: i64,
    account_id: i64,
    title: String,
    public_key: String,
    algorithm: String,
    fingerprint: String,
    created_at: i64,
    last_used_at: Option<i64>,
}

impl From<SshKeyRow> for SshKey {
    fn from(row: SshKeyRow) -> Self {
        Self {
            id: SshKeyId(row.id),
            account_id: AccountId(row.account_id),
            title: row.title,
            public_key: row.public_key,
            algorithm: row.algorithm,
            fingerprint: row.fingerprint,
            created_at: Timestamp(row.created_at),
            last_used_at: row.last_used_at.map(Timestamp),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct DeployKeyRow {
    id: i64,
    repo_id: i64,
    title: String,
    public_key: String,
    algorithm: String,
    fingerprint: String,
    read_only: bool,
    created_at: i64,
    last_used_at: Option<i64>,
}

impl From<DeployKeyRow> for DeployKey {
    fn from(row: DeployKeyRow) -> Self {
        Self {
            id: DeployKeyId(row.id),
            repo_id: RepoId(row.repo_id),
            title: row.title,
            public_key: row.public_key,
            algorithm: row.algorithm,
            fingerprint: row.fingerprint,
            read_only: row.read_only,
            created_at: Timestamp(row.created_at),
            last_used_at: row.last_used_at.map(Timestamp),
        }
    }
}

/// A user key joined with its owner (key columns prefixed `key_`).
#[derive(Debug, sqlx::FromRow)]
struct SshKeyOwnerRow {
    #[sqlx(flatten)]
    user: UserRow,
    suspended: bool,
    key_id: i64,
    key_title: String,
    key_public_key: String,
    key_algorithm: String,
    key_fingerprint: String,
    key_created_at: i64,
    key_last_used_at: Option<i64>,
}

/// A deploy key joined with its repository (key columns prefixed `key_`).
#[derive(Debug, sqlx::FromRow)]
struct DeployKeyRepoRow {
    #[sqlx(flatten)]
    repo: OwnedRepositoryRow,
    key_id: i64,
    key_title: String,
    key_public_key: String,
    key_algorithm: String,
    key_fingerprint: String,
    key_read_only: bool,
    key_created_at: i64,
    key_last_used_at: Option<i64>,
}

/// Builds a static `SELECT` over a key table with the given trailing clause.
/// SQL stays a compile-time literal, so it can never contain interpolated user input.
macro_rules! select_ssh_key {
    ($clause:literal) => {
        concat!("SELECT id, account_id, title, public_key, algorithm, fingerprint, created_at, last_used_at FROM ssh_keys ", $clause)
    };
}

macro_rules! select_deploy_key {
    ($clause:literal) => {
        concat!(
            "SELECT id, repo_id, title, public_key, algorithm, fingerprint, read_only, created_at, last_used_at FROM deploy_keys ",
            $clause
        )
    };
}

/// Maps both kinds of duplicate (same table: UNIQUE; other table: trigger) to a conflict.
fn insert_error(error: sqlx::Error) -> StorageError {
    if let sqlx::Error::Database(db) = &error
        && (db.is_unique_violation() || db.message().contains("UNIQUE constraint failed: ssh key fingerprint"))
    {
        return StorageError::Conflict(KEY_FIELD);
    }
    StorageError::Sqlite(error)
}

#[derive(Debug, Clone)]
pub struct SshKeyStore {
    pool: SqlitePool,
}

impl SshKeyStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // -------------------------------------------------------------- user keys

    /// [`StorageError::Conflict`] when the fingerprint is already registered anywhere.
    pub async fn add_user_key(&self, account_id: AccountId, new: &NewPublicKey) -> Result<SshKey, StorageError> {
        let row: SshKeyRow = sqlx::query_as(
            "INSERT INTO ssh_keys (account_id, title, public_key, algorithm, fingerprint, created_at) VALUES (?, ?, ?, ?, ?, ?) \
             RETURNING id, account_id, title, public_key, algorithm, fingerprint, created_at, last_used_at",
        )
        .bind(account_id.0)
        .bind(&new.title)
        .bind(&new.public_key)
        .bind(&new.algorithm)
        .bind(&new.fingerprint)
        .bind(Timestamp::now().seconds())
        .fetch_one(&self.pool)
        .await
        .map_err(insert_error)?;
        Ok(row.into())
    }

    /// The user's keys, newest first.
    pub async fn list_user_keys(&self, account_id: AccountId) -> Result<Vec<SshKey>, StorageError> {
        let rows: Vec<SshKeyRow> = sqlx::query_as(select_ssh_key!("WHERE account_id = ? ORDER BY created_at DESC, id DESC"))
            .bind(account_id.0)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(SshKey::from).collect())
    }

    pub async fn count_user_keys(&self, account_id: AccountId) -> Result<i64, StorageError> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM ssh_keys WHERE account_id = ?").bind(account_id.0).fetch_one(&self.pool).await?)
    }

    /// Deletes the key only if `account_id` owns it. Returns whether a key was removed.
    pub async fn delete_user_key(&self, account_id: AccountId, id: SshKeyId) -> Result<bool, StorageError> {
        let result =
            sqlx::query("DELETE FROM ssh_keys WHERE id = ? AND account_id = ?").bind(id.0).bind(account_id.0).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    /// Records a use at `now`, unless one was already recorded within the last hour.
    pub async fn touch_user_key(&self, id: SshKeyId, now: Timestamp) -> Result<(), StorageError> {
        sqlx::query("UPDATE ssh_keys SET last_used_at = ?1 WHERE id = ?2 AND (last_used_at IS NULL OR last_used_at <= ?1 - ?3)")
            .bind(now.seconds())
            .bind(id.0)
            .bind(LAST_USED_RESOLUTION_SECONDS)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // -------------------------------------------------------------- deploy keys

    /// [`StorageError::Conflict`] when the fingerprint is already registered anywhere.
    pub async fn add_deploy_key(&self, repo_id: RepoId, new: &NewPublicKey, read_only: bool) -> Result<DeployKey, StorageError> {
        let row: DeployKeyRow = sqlx::query_as(
            "INSERT INTO deploy_keys (repo_id, title, public_key, algorithm, fingerprint, read_only, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             RETURNING id, repo_id, title, public_key, algorithm, fingerprint, read_only, created_at, last_used_at",
        )
        .bind(repo_id.0)
        .bind(&new.title)
        .bind(&new.public_key)
        .bind(&new.algorithm)
        .bind(&new.fingerprint)
        .bind(read_only)
        .bind(Timestamp::now().seconds())
        .fetch_one(&self.pool)
        .await
        .map_err(insert_error)?;
        Ok(row.into())
    }

    /// The repository's deploy keys, newest first.
    pub async fn list_deploy_keys(&self, repo_id: RepoId) -> Result<Vec<DeployKey>, StorageError> {
        let rows: Vec<DeployKeyRow> = sqlx::query_as(select_deploy_key!("WHERE repo_id = ? ORDER BY created_at DESC, id DESC"))
            .bind(repo_id.0)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(DeployKey::from).collect())
    }

    pub async fn count_deploy_keys(&self, repo_id: RepoId) -> Result<i64, StorageError> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM deploy_keys WHERE repo_id = ?").bind(repo_id.0).fetch_one(&self.pool).await?)
    }

    /// Deletes the key only if it belongs to `repo_id`. Returns whether a key was removed.
    pub async fn delete_deploy_key(&self, repo_id: RepoId, id: DeployKeyId) -> Result<bool, StorageError> {
        let result =
            sqlx::query("DELETE FROM deploy_keys WHERE id = ? AND repo_id = ?").bind(id.0).bind(repo_id.0).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    /// Records a use at `now`, unless one was already recorded within the last hour.
    pub async fn touch_deploy_key(&self, id: DeployKeyId, now: Timestamp) -> Result<(), StorageError> {
        sqlx::query("UPDATE deploy_keys SET last_used_at = ?1 WHERE id = ?2 AND (last_used_at IS NULL OR last_used_at <= ?1 - ?3)")
            .bind(now.seconds())
            .bind(id.0)
            .bind(LAST_USED_RESOLUTION_SECONDS)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // -------------------------------------------------------------- sign-in

    /// Resolves a `SHA256:…` fingerprint to the user or repository holding the key.
    pub async fn find_by_fingerprint(&self, fingerprint: &str) -> Result<Option<KeyHolder>, StorageError> {
        let user: Option<SshKeyOwnerRow> = sqlx::query_as(
            "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at, l.email, l.is_admin, l.password_hash, \
             l.suspended_at IS NOT NULL AS suspended, \
             k.id AS key_id, k.title AS key_title, k.public_key AS key_public_key, k.algorithm AS key_algorithm, \
             k.fingerprint AS key_fingerprint, k.created_at AS key_created_at, k.last_used_at AS key_last_used_at \
             FROM ssh_keys k JOIN accounts a ON a.id = k.account_id JOIN user_logins l ON l.account_id = a.id \
             WHERE k.fingerprint = ?",
        )
        .bind(fingerprint)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(row) = user {
            let key = SshKey {
                id: SshKeyId(row.key_id),
                account_id: AccountId(row.user.account.id),
                title: row.key_title,
                public_key: row.key_public_key,
                algorithm: row.key_algorithm,
                fingerprint: row.key_fingerprint,
                created_at: Timestamp(row.key_created_at),
                last_used_at: row.key_last_used_at.map(Timestamp),
            };
            let (user, _password_hash) = row.user.into_user_and_hash()?;
            return Ok(Some(KeyHolder::User { key, user, suspended: row.suspended }));
        }

        let deploy: Option<DeployKeyRepoRow> = sqlx::query_as(
            "SELECT r.id, r.owner_id, r.name, r.description, r.visibility, r.default_branch, r.created_at, r.updated_at, \
             r.pushed_at, (SELECT a.archived_at FROM repo_archives a WHERE a.repo_id = r.id) AS archived_at, \
             o.kind AS owner_kind, o.name AS owner_name, o.display_name AS owner_display_name, \
             o.bio AS owner_bio, o.created_at AS owner_created_at, \
             k.id AS key_id, k.title AS key_title, k.public_key AS key_public_key, k.algorithm AS key_algorithm, \
             k.fingerprint AS key_fingerprint, k.read_only AS key_read_only, k.created_at AS key_created_at, \
             k.last_used_at AS key_last_used_at \
             FROM deploy_keys k JOIN repositories r ON r.id = k.repo_id JOIN accounts o ON o.id = r.owner_id \
             WHERE k.fingerprint = ?",
        )
        .bind(fingerprint)
        .fetch_optional(&self.pool)
        .await?;
        deploy
            .map(|row| {
                let key = DeployKey {
                    id: DeployKeyId(row.key_id),
                    repo_id: RepoId(row.repo.repo.id),
                    title: row.key_title,
                    public_key: row.key_public_key,
                    algorithm: row.key_algorithm,
                    fingerprint: row.key_fingerprint,
                    read_only: row.key_read_only,
                    created_at: Timestamp(row.key_created_at),
                    last_used_at: row.key_last_used_at.map(Timestamp),
                };
                Ok(KeyHolder::Deploy { key, repo: row.repo.try_into()? })
            })
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::Visibility;

    use super::*;
    use crate::{Database, NewRepository, NewUser, StorageError};

    async fn open() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        (dir, database)
    }

    async fn user(database: &Database, name: &str) -> User {
        database
            .accounts()
            .create_user(NewUser {
                name: name.parse().unwrap(),
                email: format!("{name}@example.com").parse().unwrap(),
                display_name: None,
                password_hash: "not-a-real-hash".to_owned(),
                is_admin: false,
            })
            .await
            .unwrap()
    }

    async fn repo(database: &Database, owner: &User, name: &str) -> RepoId {
        let new = NewRepository {
            owner_id: owner.id(),
            name: name.parse().unwrap(),
            description: None,
            visibility: Visibility::Private,
            default_branch: "main".to_owned(),
        };
        database.repositories().create(new).await.unwrap().id
    }

    fn key(fingerprint: &str) -> NewPublicKey {
        NewPublicKey {
            title: "laptop".to_owned(),
            public_key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample".to_owned(),
            algorithm: "ssh-ed25519".to_owned(),
            fingerprint: fingerprint.to_owned(),
        }
    }

    #[tokio::test]
    async fn user_keys_round_trip_and_resolve_to_their_owner() {
        let (_dir, database) = open().await;
        let store = database.ssh_keys();
        let alice = user(&database, "alice").await;
        let bob = user(&database, "bob").await;

        let first = store.add_user_key(alice.id(), &key("SHA256:one")).await.unwrap();
        let second = store.add_user_key(alice.id(), &key("SHA256:two")).await.unwrap();
        assert_eq!(first.title, "laptop");
        assert!(first.last_used_at.is_none());
        let listed: Vec<_> = store.list_user_keys(alice.id()).await.unwrap().into_iter().map(|k| k.id).collect();
        assert_eq!(listed, vec![second.id, first.id], "newest first");
        assert_eq!(store.count_user_keys(alice.id()).await.unwrap(), 2);

        let Some(KeyHolder::User { key, user, suspended }) = store.find_by_fingerprint("SHA256:one").await.unwrap() else {
            panic!("expected a user key");
        };
        assert_eq!(key.id, first.id);
        assert_eq!(user.id(), alice.id());
        assert!(!suspended);
        assert!(store.find_by_fingerprint("SHA256:unknown").await.unwrap().is_none());

        store.touch_user_key(first.id, Timestamp(1_000_000)).await.unwrap();
        let touched = store.list_user_keys(alice.id()).await.unwrap().into_iter().find(|k| k.id == first.id).unwrap();
        assert_eq!(touched.last_used_at, Some(Timestamp(1_000_000)));

        assert!(!store.delete_user_key(bob.id(), first.id).await.unwrap(), "only the owner may delete");
        assert!(store.delete_user_key(alice.id(), first.id).await.unwrap());
        assert!(store.find_by_fingerprint("SHA256:one").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn suspension_is_reported_with_the_key() {
        let (_dir, database) = open().await;
        let alice = user(&database, "alice").await;
        database.ssh_keys().add_user_key(alice.id(), &key("SHA256:one")).await.unwrap();
        database.admin().set_suspended(alice.id(), true).await.unwrap();

        let holder = database.ssh_keys().find_by_fingerprint("SHA256:one").await.unwrap();
        assert!(matches!(holder, Some(KeyHolder::User { suspended: true, .. })));
    }

    #[tokio::test]
    async fn deploy_keys_resolve_to_their_repository() {
        let (_dir, database) = open().await;
        let store = database.ssh_keys();
        let alice = user(&database, "alice").await;
        let project = repo(&database, &alice, "project").await;
        let other = repo(&database, &alice, "other").await;

        let added = store.add_deploy_key(project, &key("SHA256:deploy"), false).await.unwrap();
        assert!(!added.read_only);
        assert_eq!(store.list_deploy_keys(project).await.unwrap(), vec![added.clone()]);
        assert_eq!(store.count_deploy_keys(other).await.unwrap(), 0);

        let Some(KeyHolder::Deploy { key, repo }) = store.find_by_fingerprint("SHA256:deploy").await.unwrap() else {
            panic!("expected a deploy key");
        };
        assert_eq!(key.id, added.id);
        assert_eq!(repo.repo.id, project);
        assert_eq!(repo.owner.name.as_str(), "alice");

        assert!(!store.delete_deploy_key(other, added.id).await.unwrap(), "keys are scoped to their repository");
        assert!(store.delete_deploy_key(project, added.id).await.unwrap());
    }

    #[tokio::test]
    async fn a_fingerprint_is_unique_across_both_tables() {
        let (_dir, database) = open().await;
        let store = database.ssh_keys();
        let alice = user(&database, "alice").await;
        let bob = user(&database, "bob").await;
        let project = repo(&database, &alice, "project").await;

        store.add_user_key(alice.id(), &key("SHA256:one")).await.unwrap();
        assert!(matches!(store.add_user_key(bob.id(), &key("SHA256:one")).await, Err(StorageError::Conflict(_))));
        assert!(matches!(store.add_deploy_key(project, &key("SHA256:one"), true).await, Err(StorageError::Conflict(_))));

        store.add_deploy_key(project, &key("SHA256:two"), true).await.unwrap();
        assert!(matches!(store.add_user_key(bob.id(), &key("SHA256:two")).await, Err(StorageError::Conflict(_))));
    }
}
