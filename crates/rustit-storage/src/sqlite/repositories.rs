//! Repository metadata, the shared "readable by" predicate and per-viewer grants.
//!
//! Who may read a repository is decided by `rustit_auth::policy::repo_access`. Listings
//! that must filter in SQL use [`readable_by!`], the same rules expressed once as a
//! `'static` SQL fragment; `rustit-services/tests/access.rs` checks that the two agree on
//! a fixture matrix.

use rustit_core::{
    Account, AccountId, OrgRole, RepoId, RepoName, RepoSort, Repository, Timestamp, Visibility,
    membership::{BasePermission, Permission},
    repository::OwnedRepository,
};
use sqlx::SqlitePool;

use super::{
    collaborators::{ViewerGrants, parse_permission},
    rows::{AccountRow, OwnedRepositoryRow, RepositoryRow},
};
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

/// Whether the viewer whose id is bound at `$viewer` (a numbered parameter such as
/// `"?2"`, `NULL` when anonymous) may read repository `r`; `$is_admin` is a boolean
/// parameter (or literal) saying whether that viewer is a site administrator.
///
/// The rules of `rustit_auth::policy::repo_access` reduced to "access ≥ read": public,
/// site admin, personal owner, collaborator with any permission, or a member of the
/// owning organisation who is an owner, has a base permission other than `none`, or is
/// on a team of that organisation that was granted the repository.
macro_rules! readable_by {
    ($viewer:literal, $is_admin:literal) => {
        concat!(
            "(r.visibility = 'public' OR ",
            $is_admin,
            " OR r.owner_id = ",
            $viewer,
            " OR EXISTS (SELECT 1 FROM collaborators c WHERE c.repo_id = r.id AND c.user_id = ",
            $viewer,
            ") OR EXISTS (SELECT 1 FROM org_members m LEFT JOIN org_profiles p ON p.account_id = m.org_id \
             WHERE m.org_id = r.owner_id AND m.user_id = ",
            $viewer,
            " AND (m.role = 'owner' OR IFNULL(p.member_permission, 'write') <> 'none' \
             OR EXISTS (SELECT 1 FROM team_repositories tr JOIN team_members tm ON tm.team_id = tr.team_id \
             JOIN teams t ON t.id = tr.team_id WHERE tr.repo_id = r.id AND t.org_id = r.owner_id AND tm.user_id = ",
            $viewer,
            "))))"
        )
    };
}
pub(crate) use readable_by;

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

    /// Repositories owned by `owner` that `viewer` may read, most recently updated first.
    pub async fn list_by_owner(
        &self,
        owner: AccountId,
        viewer: Option<AccountId>,
        viewer_is_admin: bool,
    ) -> Result<Vec<OwnedRepository>, StorageError> {
        let rows: Vec<OwnedRepositoryRow> =
            sqlx::query_as(select_owned_repo!("WHERE r.owner_id = ?1 AND ", readable_by!("?2", "?3"), " ORDER BY r.updated_at DESC"))
                .bind(owner.0)
                .bind(viewer.map(|id| id.0))
                .bind(viewer_is_admin)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(OwnedRepository::try_from).collect()
    }

    /// The dashboard list: repositories owned by `user`, by an organisation they belong
    /// to, or shared with them as a collaborator — each only if they may read it — most
    /// recently updated first. (Team grants are on organisation repositories, so they are
    /// covered by the membership clause.)
    pub async fn list_for_member(&self, user: AccountId, limit: u32) -> Result<Vec<OwnedRepository>, StorageError> {
        let rows: Vec<OwnedRepositoryRow> = sqlx::query_as(select_owned_repo!(
            "WHERE (r.owner_id = ?1 OR r.owner_id IN (SELECT org_id FROM org_members WHERE user_id = ?1) \
             OR r.id IN (SELECT repo_id FROM collaborators WHERE user_id = ?1)) AND ",
            readable_by!("?1", "0"),
            " ORDER BY r.updated_at DESC, r.id DESC LIMIT ?2"
        ))
        .bind(user.0)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(OwnedRepository::try_from).collect()
    }

    /// Everything `viewer` was granted on repository `repo` (owned by `owner`), in one query.
    pub async fn grants(&self, repo: RepoId, owner: AccountId, viewer: AccountId) -> Result<ViewerGrants, StorageError> {
        let (role, base, team, collaborator): (Option<String>, Option<String>, Option<i64>, Option<String>) = sqlx::query_as(
            "SELECT (SELECT role FROM org_members WHERE org_id = ?1 AND user_id = ?3), \
             (SELECT member_permission FROM org_profiles WHERE account_id = ?1), \
             (SELECT MAX(CASE tr.permission WHEN 'admin' THEN 3 WHEN 'write' THEN 2 ELSE 1 END) \
              FROM team_repositories tr JOIN team_members tm ON tm.team_id = tr.team_id JOIN teams t ON t.id = tr.team_id \
              WHERE tr.repo_id = ?2 AND t.org_id = ?1 AND tm.user_id = ?3), \
             (SELECT permission FROM collaborators WHERE repo_id = ?2 AND user_id = ?3)",
        )
        .bind(owner.0)
        .bind(repo.0)
        .bind(viewer.0)
        .fetch_one(&self.pool)
        .await?;
        let org_role = match role.as_deref() {
            None => None,
            Some(value) => Some(OrgRole::parse(value).ok_or_else(|| StorageError::Corrupt(format!("organization role: {value}")))?),
        };
        let org_base = match base.as_deref() {
            None => BasePermission::default(),
            Some(value) => BasePermission::parse(value).ok_or_else(|| StorageError::Corrupt(format!("base permission: {value}")))?,
        };
        let team = match team {
            None => None,
            Some(3) => Some(Permission::Admin),
            Some(2) => Some(Permission::Write),
            Some(_) => Some(Permission::Read),
        };
        Ok(ViewerGrants { org_role, org_base, team, collaborator: collaborator.as_deref().map(parse_permission).transpose()? })
    }

    /// Moves repository `repo` to `new_owner` in one transaction: the owner changes, team
    /// grants (which belong to the old organisation) are dropped, and a collaborator entry
    /// of the new owner, now redundant, is removed. Collaborators, stars and everything
    /// else that references the id stay. [`StorageError::Conflict`] (`"repository name"`)
    /// if the new owner already has a repository with this name.
    pub async fn transfer(&self, repo: RepoId, new_owner: AccountId) -> Result<(), StorageError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE repositories SET owner_id = ?, updated_at = ? WHERE id = ?")
            .bind(new_owner.0)
            .bind(Timestamp::now().seconds())
            .bind(repo.0)
            .execute(&mut *tx)
            .await
            .map_err(|e| StorageError::from_insert(e, "repository name"))?;
        sqlx::query("DELETE FROM team_repositories WHERE repo_id = ?").bind(repo.0).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM collaborators WHERE repo_id = ? AND user_id = ?").bind(repo.0).bind(new_owner.0).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Makes `user` a collaborator on `repo` with `permission` (or changes their
    /// permission): used to give the creator of an organisation repository admin access.
    pub async fn grant_collaborator(&self, repo: RepoId, user: AccountId, permission: Permission) -> Result<(), StorageError> {
        super::collaborators::upsert(&self.pool, repo, user, permission, Some(user)).await
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
