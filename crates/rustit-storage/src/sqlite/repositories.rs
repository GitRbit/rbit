//! Repository metadata.

use rustit_core::{
    Account, AccountId, OrgRole, RepoId, RepoName, RepoSort, Repository, Timestamp, Visibility, repository::OwnedRepository,
};
use sqlx::SqlitePool;

use super::rows::{AccountRow, OwnedRepositoryRow, RepositoryRow};
use crate::StorageError;

/// Builds a static `SELECT` of repositories joined with their owner, with owner columns
/// aliased to match [`OwnedRepositoryRow`]. Takes one or more trailing SQL fragments.
macro_rules! select_owned_repo {
    ($($clause:expr),+) => {
        concat!(
            "SELECT r.id, r.owner_id, r.name, r.description, r.visibility, r.default_branch, r.created_at, r.updated_at, \
             r.pushed_at, o.kind AS owner_kind, o.name AS owner_name, o.display_name AS owner_display_name, \
             o.bio AS owner_bio, o.created_at AS owner_created_at \
             FROM repositories r JOIN accounts o ON o.id = r.owner_id ",
            $($clause),+
        )
    };
}

/// Explore filter: public repositories whose name or description matches the `LIKE`
/// pattern `?1` (escape character `\`).
macro_rules! public_matching {
    () => {
        r"WHERE r.visibility = 'public' AND (r.name LIKE ?1 ESCAPE '\' OR IFNULL(r.description, '') LIKE ?1 ESCAPE '\') "
    };
}

/// Explore query for one sort order; `?2` is the limit and `?3` the offset.
macro_rules! search_public {
    ($order:literal) => {
        select_owned_repo!(public_matching!(), $order, " LIMIT ?2 OFFSET ?3")
    };
}

/// Turns free text into a `LIKE` pattern matching it anywhere, with wildcards escaped.
fn contains_pattern(text: &str) -> String {
    let mut pattern = String::with_capacity(text.len() + 2);
    pattern.push('%');
    for c in text.chars() {
        if matches!(c, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

#[derive(Debug)]
pub struct NewRepository {
    pub owner_id: AccountId,
    pub name: RepoName,
    pub description: Option<String>,
    pub visibility: Visibility,
    pub default_branch: String,
}

#[derive(Debug, Clone)]
pub struct RepositoryStore {
    pool: SqlitePool,
}

impl RepositoryStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, new: NewRepository) -> Result<Repository, StorageError> {
        let now = Timestamp::now().seconds();
        let row: RepositoryRow = sqlx::query_as(
            "INSERT INTO repositories (owner_id, name, description, visibility, default_branch, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             RETURNING id, owner_id, name, description, visibility, default_branch, created_at, updated_at, pushed_at",
        )
        .bind(new.owner_id.0)
        .bind(new.name.as_str())
        .bind(new.description.as_deref())
        .bind(new.visibility.as_str())
        .bind(&new.default_branch)
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::from_insert(e, "repository name"))?;
        row.try_into()
    }

    /// Removes the metadata row. The caller is responsible for deleting the bare repository.
    pub async fn delete(&self, id: RepoId) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM repositories WHERE id = ?").bind(id.0).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn find_by_path(&self, owner: &str, name: &str) -> Result<Option<OwnedRepository>, StorageError> {
        let row: Option<OwnedRepositoryRow> =
            sqlx::query_as(select_owned_repo!("WHERE o.name = ? AND r.name = ?")).bind(owner).bind(name).fetch_optional(&self.pool).await?;
        row.map(OwnedRepository::try_from).transpose()
    }

    /// Repositories owned by `owner`, most recently updated first.
    pub async fn list_by_owner(&self, owner: AccountId, include_private: bool) -> Result<Vec<OwnedRepository>, StorageError> {
        let rows: Vec<OwnedRepositoryRow> =
            sqlx::query_as(select_owned_repo!("WHERE r.owner_id = ? AND (? OR r.visibility = 'public') ORDER BY r.updated_at DESC"))
                .bind(owner.0)
                .bind(include_private)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(OwnedRepository::try_from).collect()
    }

    /// Repositories owned by `user` or by any organisation they belong to (including
    /// private ones: members may read every repository of their organisations),
    /// most recently updated first.
    pub async fn list_for_member(&self, user: AccountId, limit: u32) -> Result<Vec<OwnedRepository>, StorageError> {
        let rows: Vec<OwnedRepositoryRow> = sqlx::query_as(select_owned_repo!(
            "WHERE r.owner_id = ?1 OR r.owner_id IN (SELECT org_id FROM org_members WHERE user_id = ?1) \
             ORDER BY r.updated_at DESC, r.id DESC LIMIT ?2"
        ))
        .bind(user.0)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(OwnedRepository::try_from).collect()
    }

    /// One page of public repositories whose name or description contains `text`
    /// (case-insensitively; empty matches everything), in `sort` order.
    pub async fn search_public(&self, text: &str, sort: RepoSort, limit: u32, offset: u32) -> Result<Vec<OwnedRepository>, StorageError> {
        let sql = match sort {
            RepoSort::RecentlyUpdated => search_public!("ORDER BY r.updated_at DESC, r.id DESC"),
            RepoSort::Name => search_public!("ORDER BY r.name COLLATE NOCASE, o.name COLLATE NOCASE, r.id"),
            RepoSort::Newest => search_public!("ORDER BY r.created_at DESC, r.id DESC"),
        };
        let rows: Vec<OwnedRepositoryRow> =
            sqlx::query_as(sql).bind(contains_pattern(text)).bind(limit).bind(offset).fetch_all(&self.pool).await?;
        rows.into_iter().map(OwnedRepository::try_from).collect()
    }

    /// How many repositories [`Self::search_public`] can page through for `text`.
    pub async fn count_public(&self, text: &str) -> Result<i64, StorageError> {
        Ok(sqlx::query_scalar(concat!("SELECT COUNT(*) FROM repositories r ", public_matching!()))
            .bind(contains_pattern(text))
            .fetch_one(&self.pool)
            .await?)
    }

    /// The user or organisation called `name`, for choosing a new repository's owner.
    pub async fn find_owner(&self, name: &str) -> Result<Option<Account>, StorageError> {
        let row: Option<AccountRow> = sqlx::query_as("SELECT id, kind, name, display_name, bio, created_at FROM accounts WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;
        row.map(Account::try_from).transpose()
    }

    /// `user`'s role in the organisation `org`, if any (membership-aware access checks).
    pub async fn org_role(&self, org: AccountId, user: AccountId) -> Result<Option<OrgRole>, StorageError> {
        super::orgs::role_of(&self.pool, org, user).await
    }

    /// Recently active public repositories for the explore page.
    pub async fn list_recent_public(&self, limit: u32) -> Result<Vec<OwnedRepository>, StorageError> {
        let rows: Vec<OwnedRepositoryRow> =
            sqlx::query_as(select_owned_repo!("WHERE r.visibility = 'public' ORDER BY r.updated_at DESC LIMIT ?"))
                .bind(limit)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(OwnedRepository::try_from).collect()
    }

    /// Records that new commits arrived.
    pub async fn mark_pushed(&self, id: RepoId) -> Result<(), StorageError> {
        let now = Timestamp::now().seconds();
        sqlx::query("UPDATE repositories SET pushed_at = ?, updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(now)
            .bind(id.0)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::contains_pattern;

    #[test]
    fn like_wildcards_are_escaped() {
        assert_eq!(contains_pattern(""), "%%");
        assert_eq!(contains_pattern("web"), "%web%");
        assert_eq!(contains_pattern(r"100%_a\b"), r"%100\%\_a\\b%");
    }
}
