//! Users, organisations and their credentials.
//!
//! Suspension is enforced here: [`AccountStore::find_login`] reports it so sign-in can be
//! refused, and [`AccountStore::find_active_user`] never resolves a suspended user.

use rbit_core::{Account, AccountId, AccountKind, AccountName, Email, Timestamp, User};
use sqlx::SqlitePool;

use super::rows::{AccountRow, UserRow};
use crate::StorageError;

/// Keep each bulk lookup small enough to share the pool with interactive requests.
const USER_QUERY_BATCH_SIZE: usize = 256;

/// Builds a static `SELECT` over users (account ⋈ login) with the given trailing clause.
/// SQL stays a compile-time literal, so it can never contain interpolated user input.
macro_rules! select_user {
    ($clause:literal) => {
        concat!(
            "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at, l.email, l.is_admin, l.password_hash \
             FROM accounts a JOIN user_logins l ON l.account_id = a.id ",
            $clause
        )
    };
}

/// A login candidate: the user, their stored password hash and whether they are suspended.
#[derive(Debug)]
pub struct LoginRecord {
    pub user: User,
    pub password_hash: String,
    pub suspended: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct LoginRow {
    #[sqlx(flatten)]
    user: UserRow,
    suspended_at: Option<i64>,
}

/// Everything needed to register a user. The password must already be hashed.
#[derive(Debug)]
pub struct NewUser {
    pub name: AccountName,
    pub email: Email,
    pub display_name: Option<String>,
    pub password_hash: String,
    pub is_admin: bool,
}

#[derive(Debug, Clone)]
pub struct AccountStore {
    pool: SqlitePool,
}

impl AccountStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The security audit log in the same database, for services that were handed only
    /// this store but must record what they do (sessions, access tokens).
    pub fn audit_log(&self) -> super::audit::AuditStore {
        super::audit::AuditStore::new(self.pool.clone())
    }

    /// Creates the account and its login atomically.
    pub async fn create_user(&self, new: NewUser) -> Result<User, StorageError> {
        let mut tx = self.pool.begin().await?;
        let user = insert_user(&mut tx, new, None, false).await?;
        tx.commit().await?;
        Ok(user)
    }

    /// Creates an administrator-provisioned account with a temporary password.
    pub async fn create_user_with_initial_password(&self, new: NewUser) -> Result<User, StorageError> {
        let mut tx = self.pool.begin().await?;
        let user = insert_user(&mut tx, new, None, false).await?;
        sqlx::query("UPDATE user_logins SET password_change_required = 1 WHERE account_id = ?").bind(user.id().0).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(user)
    }

    /// Whether the account must replace its administrator-provided password.
    pub async fn password_change_required(&self, id: AccountId) -> Result<bool, StorageError> {
        Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM user_logins WHERE account_id = ? AND password_change_required = 1)")
            .bind(id.0)
            .fetch_one(&self.pool)
            .await?)
    }

    /// Registers a user, deciding the first administrator inside the write transaction.
    /// Closed registration admits only the first user; `new.is_admin` is ignored.
    pub async fn register_user(&self, new: NewUser, registration_open: bool) -> Result<User, StorageError> {
        let mut tx = self.pool.begin().await?;
        let user = insert_user(&mut tx, new, Some(registration_open), false).await?;
        tx.commit().await?;
        Ok(user)
    }

    /// Looks a user up by username *or* email, returning the stored password hash for
    /// verification and whether the account is suspended.
    pub async fn find_login(&self, username_or_email: &str) -> Result<Option<LoginRecord>, StorageError> {
        let row: Option<LoginRow> = sqlx::query_as(
            "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at, l.email, l.is_admin, l.password_hash, \
             l.suspended_at FROM accounts a JOIN user_logins l ON l.account_id = a.id \
             WHERE a.name = ?1 OR l.email = ?1",
        )
        .bind(username_or_email)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            let suspended = row.suspended_at.is_some();
            row.user.into_user_and_hash().map(|(user, password_hash)| LoginRecord { user, password_hash, suspended })
        })
        .transpose()
    }

    pub async fn find_user(&self, id: AccountId) -> Result<Option<User>, StorageError> {
        let row: Option<UserRow> = sqlx::query_as(select_user!("WHERE a.id = ?")).bind(id.0).fetch_optional(&self.pool).await?;
        Ok(row.map(UserRow::into_user_and_hash).transpose()?.map(|(user, _)| user))
    }

    /// Like [`Self::find_user`], but `None` for suspended users. Use it wherever a
    /// credential (session, token) is turned back into a user.
    pub async fn find_active_user(&self, id: AccountId) -> Result<Option<User>, StorageError> {
        let row: Option<UserRow> =
            sqlx::query_as(select_user!("WHERE a.id = ? AND l.suspended_at IS NULL")).bind(id.0).fetch_optional(&self.pool).await?;
        Ok(row.map(UserRow::into_user_and_hash).transpose()?.map(|(user, _)| user))
    }

    /// Users among `ids`, including suspended users, as in [`Self::find_user`].
    /// Unknown IDs and duplicates are omitted; results are ordered by account ID.
    pub async fn find_users(&self, ids: &[AccountId]) -> Result<Vec<User>, StorageError> {
        self.users_by_ids(ids, false).await
    }

    /// Active users among `ids`, applying the same suspension check as
    /// [`Self::find_active_user`] without one database round trip per recipient.
    pub async fn find_active_users(&self, ids: &[AccountId]) -> Result<Vec<User>, StorageError> {
        self.users_by_ids(ids, true).await
    }

    async fn users_by_ids(&self, ids: &[AccountId], active_only: bool) -> Result<Vec<User>, StorageError> {
        let mut ids: Vec<i64> = ids.iter().map(|id| id.0).collect();
        ids.sort_unstable();
        ids.dedup();
        let mut users = Vec::new();
        for batch in ids.chunks(USER_QUERY_BATCH_SIZE) {
            let rows: Vec<UserRow> = sqlx::query_as(
                "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at, l.email, l.is_admin, '' AS password_hash \
                 FROM accounts a JOIN user_logins l ON l.account_id = a.id \
                 WHERE a.id IN (SELECT value FROM json_each(?1)) AND (?2 = 0 OR l.suspended_at IS NULL) ORDER BY a.id",
            )
            .bind(format!("{batch:?}"))
            .bind(active_only)
            .fetch_all(&self.pool)
            .await?;
            for row in rows {
                users.push(row.into_user_and_hash()?.0);
            }
        }
        Ok(users)
    }

    /// Whether the user is suspended. Unknown ids are reported as not suspended.
    pub async fn is_suspended(&self, id: AccountId) -> Result<bool, StorageError> {
        Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM user_logins WHERE account_id = ? AND suspended_at IS NOT NULL)")
            .bind(id.0)
            .fetch_one(&self.pool)
            .await?)
    }

    pub async fn find_account_by_name(&self, name: &str) -> Result<Option<Account>, StorageError> {
        let row: Option<AccountRow> = sqlx::query_as("SELECT id, kind, name, display_name, bio, created_at FROM accounts WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;
        row.map(Account::try_from).transpose()
    }

    /// The accounts among `ids` (users and organisations; unknown ids are left out), in
    /// one query.
    pub async fn find_accounts(&self, ids: &[AccountId]) -> Result<Vec<Account>, StorageError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<i64> = ids.iter().map(|id| id.0).collect();
        let rows: Vec<AccountRow> = sqlx::query_as(
            "SELECT id, kind, name, display_name, bio, created_at FROM accounts WHERE id IN (SELECT value FROM json_each(?)) ORDER BY id",
        )
        .bind(format!("{ids:?}"))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Account::try_from).collect()
    }

    pub async fn count_users(&self) -> Result<i64, StorageError> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM user_logins").fetch_one(&self.pool).await?)
    }
}

/// Shared by password and external registration so the bootstrap decision is serialized
/// across both paths. The first INSERT acquires SQLite's write lock before we inspect
/// the logins. No password hashing or other slow work runs while holding the lock.
pub(super) async fn insert_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    mut new: NewUser,
    registration_open: Option<bool>,
    email_verified: bool,
) -> Result<User, StorageError> {
    let now = Timestamp::now();
    let id: i64 =
        sqlx::query_scalar("INSERT INTO accounts (kind, name, display_name, created_at, updated_at) VALUES (?, ?, ?, ?, ?) RETURNING id")
            .bind(AccountKind::User.as_str())
            .bind(new.name.as_str())
            .bind(new.display_name.as_deref())
            .bind(now.0)
            .bind(now.0)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| StorageError::from_insert(e, "username"))?;

    if let Some(open) = registration_open {
        let first: bool = sqlx::query_scalar("SELECT NOT EXISTS (SELECT 1 FROM user_logins)").fetch_one(&mut **tx).await?;
        if !open && !first {
            return Err(StorageError::RegistrationClosed);
        }
        new.is_admin = first;
    }
    sqlx::query("INSERT INTO user_logins (account_id, email, password_hash, is_admin, email_verified_at) VALUES (?, ?, ?, ?, ?)")
        .bind(id)
        .bind(new.email.as_str())
        .bind(&new.password_hash)
        .bind(new.is_admin)
        .bind(email_verified.then_some(now.0))
        .execute(&mut **tx)
        .await
        .map_err(|e| StorageError::from_insert(e, "email"))?;

    Ok(User {
        account: Account {
            id: AccountId(id),
            kind: AccountKind::User,
            name: new.name,
            display_name: new.display_name,
            bio: None,
            created_at: now,
        },
        email: new.email,
        is_admin: new.is_admin,
    })
}
