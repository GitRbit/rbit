//! Instance-wide queries for the administration panel (counts, listings, moderation).
//!
//! Moderation statements carry the "never remove the last active administrator" rule in
//! their `WHERE` clause, so the check and the write are one atomic statement and two
//! administrators acting concurrently can never lock the instance out.

use rustit_core::{Account, AccountId, Email, OwnedRepository, RepoId, Timestamp, User};
use sqlx::SqlitePool;

use super::rows::{AccountRow, OwnedRepositoryRow};
use crate::StorageError;

/// `true` unless account `?1` is the only administrator who is not suspended.
/// Shared by every statement that could remove an administrator.
macro_rules! not_last_admin {
    () => {
        "NOT EXISTS (SELECT 1 FROM user_logins t WHERE t.account_id = ?1 AND t.is_admin = 1 AND t.suspended_at IS NULL \
         AND NOT EXISTS (SELECT 1 FROM user_logins o WHERE o.is_admin = 1 AND o.suspended_at IS NULL AND o.account_id <> ?1))"
    };
}

/// Matches rows whose search columns contain `?2`, or every row when `?1` is empty.
macro_rules! search_filter {
    ($($column:literal),+) => {
        concat!("(?1 = ''", $(" OR ", $column, " LIKE ?2 ESCAPE '\\'",)+ ")")
    };
}

/// Instance-wide counts for the dashboard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct InstanceCounts {
    pub users: i64,
    pub admins: i64,
    pub suspended: i64,
    pub organizations: i64,
    pub public_repositories: i64,
    pub private_repositories: i64,
    pub access_tokens: i64,
}

/// A user as listed in the administration panel.
#[derive(Debug, Clone)]
pub struct UserRecord {
    pub user: User,
    pub suspended_at: Option<Timestamp>,
    pub repository_count: i64,
}

impl UserRecord {
    pub fn is_suspended(&self) -> bool {
        self.suspended_at.is_some()
    }
}

/// An organisation as listed in the administration panel.
#[derive(Debug, Clone)]
pub struct OrgRecord {
    pub account: Account,
    pub member_count: i64,
    pub repository_count: i64,
}

/// One page of a listing plus the total number of matching rows.
#[derive(Debug, Clone)]
pub struct Listing<T> {
    pub items: Vec<T>,
    pub total: i64,
}

/// Which page of a listing to fetch, and an optional case-insensitive substring filter.
#[derive(Debug, Clone, Copy)]
pub struct ListFilter<'a> {
    pub search: &'a str,
    pub limit: i64,
    pub offset: i64,
}

impl ListFilter<'_> {
    /// The `LIKE` pattern for the search text, with wildcards in the input escaped.
    fn pattern(&self) -> String {
        let mut pattern = String::with_capacity(self.search.len() + 2);
        pattern.push('%');
        for c in self.search.chars() {
            if matches!(c, '%' | '_' | '\\') {
                pattern.push('\\');
            }
            pattern.push(c);
        }
        pattern.push('%');
        pattern
    }
}

/// Outcome of a guarded moderation statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moderation {
    Applied,
    /// No such user.
    NotFound,
    /// Refused: the target is the last active administrator.
    LastAdmin,
}

#[derive(Debug, sqlx::FromRow)]
struct UserRecordRow {
    #[sqlx(flatten)]
    account: AccountRow,
    email: String,
    is_admin: bool,
    suspended_at: Option<i64>,
    repository_count: i64,
}

impl TryFrom<UserRecordRow> for UserRecord {
    type Error = StorageError;

    fn try_from(row: UserRecordRow) -> Result<Self, Self::Error> {
        let email: Email = row.email.parse().map_err(|e| StorageError::Corrupt(format!("email: {e}")))?;
        Ok(Self {
            user: User { account: row.account.try_into()?, email, is_admin: row.is_admin },
            suspended_at: row.suspended_at.map(Timestamp),
            repository_count: row.repository_count,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct OrgRecordRow {
    #[sqlx(flatten)]
    account: AccountRow,
    member_count: i64,
    repository_count: i64,
}

impl TryFrom<OrgRecordRow> for OrgRecord {
    type Error = StorageError;

    fn try_from(row: OrgRecordRow) -> Result<Self, Self::Error> {
        Ok(Self { account: row.account.try_into()?, member_count: row.member_count, repository_count: row.repository_count })
    }
}

/// Builds a static `SELECT` of [`UserRecordRow`]s with the given trailing clause.
macro_rules! select_user_record {
    ($($clause:tt)+) => {
        concat!(
            "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at, l.email, l.is_admin, l.suspended_at, \
             (SELECT COUNT(*) FROM repositories r WHERE r.owner_id = a.id) AS repository_count \
             FROM accounts a JOIN user_logins l ON l.account_id = a.id ",
            $($clause)+
        )
    };
}

#[derive(Debug, Clone)]
pub struct AdminStore {
    pool: SqlitePool,
}

impl AdminStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn counts(&self) -> Result<InstanceCounts, StorageError> {
        Ok(sqlx::query_as(
            "SELECT \
             (SELECT COUNT(*) FROM user_logins) AS users, \
             (SELECT COUNT(*) FROM user_logins WHERE is_admin = 1) AS admins, \
             (SELECT COUNT(*) FROM user_logins WHERE suspended_at IS NOT NULL) AS suspended, \
             (SELECT COUNT(*) FROM accounts WHERE kind = 'organization') AS organizations, \
             (SELECT COUNT(*) FROM repositories WHERE visibility = 'public') AS public_repositories, \
             (SELECT COUNT(*) FROM repositories WHERE visibility = 'private') AS private_repositories, \
             (SELECT COUNT(*) FROM access_tokens) AS access_tokens",
        )
        .fetch_one(&self.pool)
        .await?)
    }

    /// The version of the linked SQLite library, e.g. `3.50.4`.
    pub async fn sqlite_version(&self) -> Result<String, StorageError> {
        Ok(sqlx::query_scalar("SELECT sqlite_version()").fetch_one(&self.pool).await?)
    }

    // ------------------------------------------------------------------ users

    /// Users matching the filter (name, display name or email), oldest account first.
    pub async fn list_users(&self, filter: ListFilter<'_>) -> Result<Listing<UserRecord>, StorageError> {
        let pattern = filter.pattern();
        let rows: Vec<UserRecordRow> = sqlx::query_as(select_user_record!(concat!(
            "WHERE ",
            search_filter!("a.name", "a.display_name", "l.email"),
            " ORDER BY a.id LIMIT ?3 OFFSET ?4"
        )))
        .bind(filter.search)
        .bind(&pattern)
        .bind(filter.limit)
        .bind(filter.offset)
        .fetch_all(&self.pool)
        .await?;
        let total: i64 = sqlx::query_scalar(concat!(
            "SELECT COUNT(*) FROM accounts a JOIN user_logins l ON l.account_id = a.id WHERE ",
            search_filter!("a.name", "a.display_name", "l.email")
        ))
        .bind(filter.search)
        .bind(&pattern)
        .fetch_one(&self.pool)
        .await?;
        Ok(Listing { items: rows.into_iter().map(UserRecord::try_from).collect::<Result<_, _>>()?, total })
    }

    pub async fn find_user(&self, id: AccountId) -> Result<Option<UserRecord>, StorageError> {
        let row: Option<UserRecordRow> =
            sqlx::query_as(select_user_record!("WHERE a.id = ?")).bind(id.0).fetch_optional(&self.pool).await?;
        row.map(UserRecord::try_from).transpose()
    }

    /// Grants administrator rights, or revokes them unless the user is the last active administrator.
    pub async fn set_admin(&self, id: AccountId, is_admin: bool) -> Result<Moderation, StorageError> {
        let result = if is_admin {
            sqlx::query("UPDATE user_logins SET is_admin = 1 WHERE account_id = ?1").bind(id.0).execute(&self.pool).await?
        } else {
            sqlx::query(concat!("UPDATE user_logins SET is_admin = 0 WHERE account_id = ?1 AND ", not_last_admin!()))
                .bind(id.0)
                .execute(&self.pool)
                .await?
        };
        self.moderation_outcome(id, result.rows_affected()).await
    }

    /// Suspends the user (unless they are the last active administrator) or lifts a suspension.
    pub async fn set_suspended(&self, id: AccountId, suspended: bool) -> Result<Moderation, StorageError> {
        let result = if suspended {
            sqlx::query(concat!(
                "UPDATE user_logins SET suspended_at = COALESCE(suspended_at, ?2) WHERE account_id = ?1 AND ",
                not_last_admin!()
            ))
            .bind(id.0)
            .bind(Timestamp::now().seconds())
            .execute(&self.pool)
            .await?
        } else {
            sqlx::query("UPDATE user_logins SET suspended_at = NULL WHERE account_id = ?1").bind(id.0).execute(&self.pool).await?
        };
        self.moderation_outcome(id, result.rows_affected()).await
    }

    /// Deletes a user account; everything it owns in SQLite cascades. Bare repositories on
    /// disk are the caller's responsibility (see [`Self::repositories_owned_by`]).
    pub async fn delete_user(&self, id: AccountId) -> Result<Moderation, StorageError> {
        let result = sqlx::query(concat!("DELETE FROM accounts WHERE id = ?1 AND kind = 'user' AND ", not_last_admin!()))
            .bind(id.0)
            .execute(&self.pool)
            .await?;
        self.moderation_outcome(id, result.rows_affected()).await
    }

    async fn moderation_outcome(&self, id: AccountId, rows_affected: u64) -> Result<Moderation, StorageError> {
        if rows_affected > 0 {
            return Ok(Moderation::Applied);
        }
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM user_logins WHERE account_id = ?)").bind(id.0).fetch_one(&self.pool).await?;
        Ok(if exists { Moderation::LastAdmin } else { Moderation::NotFound })
    }

    /// Whether `id` is an administrator and no other administrator is active.
    pub async fn is_last_admin(&self, id: AccountId) -> Result<bool, StorageError> {
        let guarded: bool = sqlx::query_scalar(concat!("SELECT ", not_last_admin!())).bind(id.0).fetch_one(&self.pool).await?;
        Ok(!guarded)
    }

    /// Id and name of every repository the account owns, for deleting them from disk.
    pub async fn repositories_owned_by(&self, owner: AccountId) -> Result<Vec<(RepoId, String)>, StorageError> {
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, name FROM repositories WHERE owner_id = ? ORDER BY id").bind(owner.0).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|(id, name)| (RepoId(id), name)).collect())
    }

    /// Names of organisations whose only owner is `user`; deleting the user would orphan them.
    pub async fn organizations_solely_owned_by(&self, user: AccountId) -> Result<Vec<String>, StorageError> {
        Ok(sqlx::query_scalar(
            "SELECT a.name FROM org_members m JOIN accounts a ON a.id = m.org_id \
             WHERE m.user_id = ?1 AND m.role = 'owner' \
             AND NOT EXISTS (SELECT 1 FROM org_members o WHERE o.org_id = m.org_id AND o.role = 'owner' AND o.user_id <> ?1) \
             ORDER BY a.name",
        )
        .bind(user.0)
        .fetch_all(&self.pool)
        .await?)
    }

    // ------------------------------------------------------------------ organisations

    /// Organisations matching the filter (name or display name), alphabetically.
    pub async fn list_organizations(&self, filter: ListFilter<'_>) -> Result<Listing<OrgRecord>, StorageError> {
        let pattern = filter.pattern();
        let rows: Vec<OrgRecordRow> = sqlx::query_as(concat!(
            "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at, \
             (SELECT COUNT(*) FROM org_members m WHERE m.org_id = a.id) AS member_count, \
             (SELECT COUNT(*) FROM repositories r WHERE r.owner_id = a.id) AS repository_count \
             FROM accounts a WHERE a.kind = 'organization' AND ",
            search_filter!("a.name", "a.display_name"),
            " ORDER BY a.name LIMIT ?3 OFFSET ?4"
        ))
        .bind(filter.search)
        .bind(&pattern)
        .bind(filter.limit)
        .bind(filter.offset)
        .fetch_all(&self.pool)
        .await?;
        let total: i64 = sqlx::query_scalar(concat!(
            "SELECT COUNT(*) FROM accounts a WHERE a.kind = 'organization' AND ",
            search_filter!("a.name", "a.display_name")
        ))
        .bind(filter.search)
        .bind(&pattern)
        .fetch_one(&self.pool)
        .await?;
        Ok(Listing { items: rows.into_iter().map(OrgRecord::try_from).collect::<Result<_, _>>()?, total })
    }

    // ------------------------------------------------------------------ repositories

    /// Repositories (public and private) matching the filter (`owner/name` or description),
    /// most recently updated first.
    pub async fn list_repositories(&self, filter: ListFilter<'_>) -> Result<Listing<OwnedRepository>, StorageError> {
        let pattern = filter.pattern();
        let rows: Vec<OwnedRepositoryRow> = sqlx::query_as(concat!(
            "SELECT r.id, r.owner_id, r.name, r.description, r.visibility, r.default_branch, r.created_at, r.updated_at, \
             r.pushed_at, o.kind AS owner_kind, o.name AS owner_name, o.display_name AS owner_display_name, \
             o.bio AS owner_bio, o.created_at AS owner_created_at \
             FROM repositories r JOIN accounts o ON o.id = r.owner_id WHERE ",
            search_filter!("o.name || '/' || r.name", "r.description"),
            " ORDER BY r.updated_at DESC, r.id DESC LIMIT ?3 OFFSET ?4"
        ))
        .bind(filter.search)
        .bind(&pattern)
        .bind(filter.limit)
        .bind(filter.offset)
        .fetch_all(&self.pool)
        .await?;
        let total: i64 = sqlx::query_scalar(concat!(
            "SELECT COUNT(*) FROM repositories r JOIN accounts o ON o.id = r.owner_id WHERE ",
            search_filter!("o.name || '/' || r.name", "r.description")
        ))
        .bind(filter.search)
        .bind(&pattern)
        .fetch_one(&self.pool)
        .await?;
        Ok(Listing { items: rows.into_iter().map(OwnedRepository::try_from).collect::<Result<_, _>>()?, total })
    }

    pub async fn find_repository(&self, id: RepoId) -> Result<Option<OwnedRepository>, StorageError> {
        let row: Option<OwnedRepositoryRow> = sqlx::query_as(
            "SELECT r.id, r.owner_id, r.name, r.description, r.visibility, r.default_branch, r.created_at, r.updated_at, \
             r.pushed_at, o.kind AS owner_kind, o.name AS owner_name, o.display_name AS owner_display_name, \
             o.bio AS owner_bio, o.created_at AS owner_created_at \
             FROM repositories r JOIN accounts o ON o.id = r.owner_id WHERE r.id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map(OwnedRepository::try_from).transpose()
    }

    /// Removes the repository row. The caller deletes the bare repository from disk.
    pub async fn delete_repository(&self, id: RepoId) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM repositories WHERE id = ?").bind(id.0).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_pattern_escapes_wildcards() {
        let filter = ListFilter { search: "a_b%c\\", limit: 10, offset: 0 };
        assert_eq!(filter.pattern(), "%a\\_b\\%c\\\\%");
    }

    #[test]
    fn search_filter_is_static_sql() {
        assert_eq!(search_filter!("a.name"), "(?1 = '' OR a.name LIKE ?2 ESCAPE '\\')");
    }
}
