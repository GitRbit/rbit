//! Repository metadata, the shared "readable by" predicate and per-viewer grants.
//!
//! Who may read a repository is decided by `rbit_auth::policy::repo_access`. Listings
//! that must filter in SQL use [`readable_by!`], the same rules expressed once as a
//! `'static` SQL fragment; `rbit-services/tests/access.rs` checks that the two agree on
//! a fixture matrix.

use std::collections::HashMap;

use super::pool::Pool;
use rbit_core::{
    Account, AccountId, OrgRole, RepoId, RepoName, RepoSort, Repository, Timestamp, Visibility,
    membership::{BasePermission, Permission},
    mirror::Mirror,
    repository::OwnedRepository,
};

use super::{
    collaborators::{ViewerGrants, parse_permission},
    mirrors::MirrorRow,
    rows::{AccountRow, OwnedRepositoryRow, RepositoryRow},
};
use crate::StorageError;

const GRANT_QUERY_BATCH_SIZE: usize = 256;

#[derive(sqlx::FromRow)]
struct GrantsRow {
    user_id: i64,
    role: Option<String>,
    base: Option<String>,
    team: Option<i64>,
    collaborator: Option<String>,
}

fn parse_grants(
    role: Option<&str>,
    base: Option<&str>,
    team: Option<i64>,
    collaborator: Option<&str>,
) -> Result<ViewerGrants, StorageError> {
    let org_role =
        role.map(|value| OrgRole::parse(value).ok_or_else(|| StorageError::Corrupt(format!("organization role: {value}")))).transpose()?;
    let org_base = base
        .map(|value| BasePermission::parse(value).ok_or_else(|| StorageError::Corrupt(format!("base permission: {value}"))))
        .transpose()?
        .unwrap_or_default();
    let team = team.map(|rank| match rank {
        3 => Permission::Admin,
        2 => Permission::Write,
        _ => Permission::Read,
    });
    Ok(ViewerGrants { org_role, org_base, team, collaborator: collaborator.map(parse_permission).transpose()? })
}

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
/// The rules of `rbit_auth::policy::repo_access` reduced to "access ≥ read": public,
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
            ") OR EXISTS (SELECT 1 FROM org_members m WHERE m.org_id = r.owner_id AND m.user_id = ",
            $viewer,
            " AND (m.role = 'owner' \
             OR IFNULL((SELECT member_permission FROM org_profiles WHERE account_id = m.org_id), 'write') <> 'none' \
             OR EXISTS (SELECT 1 FROM team_repositories tr JOIN team_members tm ON tm.team_id = tr.team_id \
             JOIN teams t ON t.id = tr.team_id WHERE tr.repo_id = r.id AND t.org_id = r.owner_id AND tm.user_id = ",
            $viewer,
            "))))"
        )
    };
}
pub(crate) use readable_by;

/// The `WHERE` clause of the member list: repositories `r` owned by user `?1`, by an
/// organisation they belong to, or shared with them as a collaborator, each only if
/// [`readable_by!`] lets them read it (site administrators get no extra rows here).
macro_rules! member_repositories {
    () => {
        concat!(
            "WHERE (r.owner_id = ?1 OR r.owner_id IN (SELECT org_id FROM org_members WHERE user_id = ?1) \
             OR r.id IN (SELECT repo_id FROM collaborators WHERE user_id = ?1)) AND ",
            readable_by!("?1", "0")
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

/// Column `$column` of the page's owner, as `$alias`, followed by a comma.
macro_rules! page_owner {
    ($column:literal, $alias:literal) => {
        concat!("(SELECT ", $column, " FROM accounts WHERE id = page.owner_id) AS ", $alias, ", ")
    };
}

/// Column `$column` of the repository the page's repository was forked from.
macro_rules! page_parent {
    ($column:literal, $alias:literal) => {
        concat!(
            "(SELECT ",
            $column,
            " FROM repositories WHERE id = (SELECT parent_id FROM repo_forks WHERE repo_id = page.id)) AS ",
            $alias,
            ", "
        )
    };
}

/// Column `$column` of the page's mirror settings.
macro_rules! page_mirror {
    ($column:literal, $alias:literal) => {
        concat!("(SELECT ", $column, " FROM repo_mirrors WHERE repo_id = page.id) AS ", $alias, ", ")
    };
}

/// One repository page's lookup in a single statement: repository `?1/?2` with its owner
/// ([`OwnedRepositoryRow`] columns), viewer `?3`'s grants (as in
/// [`RepositoryStore::grants`]), the header's star count and whether `?3` starred it, the
/// forks `?3` may read ([`readable_by!`], `?4` being the site-administrator flag), the
/// parent it was forked from, its mirror settings and its open issue and pull request
/// counts (each an equality on `issues_by_repo (repo_id, kind, state, …)`, so only open
/// entries are read, like [`super::issues`]' `OPEN_COUNTS`).
///
/// The page's repository is `page`; [`readable_by!`] names the fork it checks `r`.
///
/// The statement has no join: `page` is the only table of the outer query, and every
/// other row is a scalar subquery on one table whose whole primary or unique key is
/// equal to a value from outside it. `SQLite` plans such a lookup by that key without
/// weighing a scan (`whereShortCut`), so the plan holds under any planner statistics.
/// As joins, `SQLite` scanned `page`, the parent and the forks' repositories whenever
/// `sqlite_stat1` recorded a one-row `repositories` table (a new instance's first
/// repository), and would keep doing so until the next `PRAGMA optimize` noticed the
/// table had grown tenfold.
pub(crate) const REPOSITORY_PAGE: &str = concat!(
    "SELECT page.id, page.owner_id, page.name, page.description, page.visibility, page.default_branch, page.created_at, page.updated_at, \
     page.pushed_at, ",
    page_owner!("kind", "owner_kind"),
    page_owner!("name", "owner_name"),
    page_owner!("display_name", "owner_display_name"),
    page_owner!("bio", "owner_bio"),
    page_owner!("created_at", "owner_created_at"),
    "(SELECT role FROM org_members WHERE org_id = page.owner_id AND user_id = ?3) AS grant_role, \
     (SELECT member_permission FROM org_profiles WHERE account_id = page.owner_id) AS grant_base, \
     (SELECT MAX(CASE tr.permission WHEN 'admin' THEN 3 WHEN 'write' THEN 2 ELSE 1 END) \
      FROM team_repositories tr JOIN team_members tm ON tm.team_id = tr.team_id JOIN teams t ON t.id = tr.team_id \
      WHERE tr.repo_id = page.id AND t.org_id = page.owner_id AND tm.user_id = ?3) AS grant_team, \
     (SELECT permission FROM collaborators WHERE repo_id = page.id AND user_id = ?3) AS grant_collaborator, \
     (SELECT COUNT(*) FROM stars WHERE repo_id = page.id) AS star_count, \
     EXISTS (SELECT 1 FROM stars WHERE account_id = ?3 AND repo_id = page.id) AS starred, \
     (SELECT COUNT(*) FROM repo_forks f INDEXED BY repo_forks_by_parent WHERE f.parent_id = page.id \
      AND (SELECT ",
    readable_by!("?3", "?4"),
    " FROM repositories r WHERE r.id = f.repo_id)) AS fork_count, \
     (SELECT parent_id FROM repo_forks WHERE repo_id = page.id) AS parent_id, ",
    page_parent!("visibility", "parent_visibility"),
    "(SELECT name FROM accounts WHERE id = (SELECT owner_id FROM repositories WHERE id = \
      (SELECT parent_id FROM repo_forks WHERE repo_id = page.id))) AS parent_owner, ",
    page_parent!("name", "parent_name"),
    page_mirror!("url", "mirror_url"),
    page_mirror!("interval_minutes", "mirror_interval_minutes"),
    page_mirror!("next_sync_at", "mirror_next_sync_at"),
    page_mirror!("last_sync_at", "mirror_last_sync_at"),
    page_mirror!("last_error", "mirror_last_error"),
    page_mirror!("created_at", "mirror_created_at"),
    "(SELECT COUNT(*) FROM issues WHERE repo_id = page.id AND kind = 'issue' AND state = 'open') AS open_issues, \
     (SELECT COUNT(*) FROM issues WHERE repo_id = page.id AND kind = 'pull' AND state = 'open') AS open_pulls \
     FROM repositories page \
     WHERE page.owner_id = (SELECT id FROM accounts WHERE name = ?1) AND page.name = ?2"
);

#[derive(sqlx::FromRow)]
struct RepositoryPageRow {
    #[sqlx(flatten)]
    repo: OwnedRepositoryRow,
    grant_role: Option<String>,
    grant_base: Option<String>,
    grant_team: Option<i64>,
    grant_collaborator: Option<String>,
    star_count: i64,
    starred: bool,
    fork_count: i64,
    parent_id: Option<i64>,
    parent_visibility: Option<String>,
    parent_owner: Option<String>,
    parent_name: Option<String>,
    mirror_url: Option<String>,
    mirror_interval_minutes: Option<i64>,
    mirror_next_sync_at: Option<i64>,
    mirror_last_sync_at: Option<i64>,
    mirror_last_error: Option<String>,
    mirror_created_at: Option<i64>,
    open_issues: i64,
    open_pulls: i64,
}

/// Everything a repository page's header needs, read by [`RepositoryStore::find_page`].
/// Nothing in it has been access-checked: the caller decides from `repo` and `grants`
/// whether the viewer may see the page at all before using any of the rest.
#[derive(Debug, Clone)]
pub struct RepositoryPage {
    pub repo: OwnedRepository,
    /// The viewer's grants on `repo`; meaningful only for a signed-in viewer who neither
    /// owns it nor administers the site (empty for anonymous viewers).
    pub grants: ViewerGrants,
    pub star_count: u64,
    pub starred_by_viewer: bool,
    /// Direct forks the viewer may read.
    pub readable_forks: u64,
    /// The repository this one was forked from, if it still exists. Not access-checked.
    pub parent: Option<ForkParent>,
    pub mirror: Option<Mirror>,
    pub open_issues: u64,
    pub open_pulls: u64,
}

/// The parent of a fork, as [`RepositoryPage`] names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkParent {
    pub id: RepoId,
    pub visibility: Visibility,
    pub owner: String,
    pub name: String,
}

impl ForkParent {
    /// `owner/name`.
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

fn corrupt(what: &str) -> StorageError {
    StorageError::Corrupt(format!("repository page: {what}"))
}

impl TryFrom<RepositoryPageRow> for RepositoryPage {
    type Error = StorageError;

    fn try_from(row: RepositoryPageRow) -> Result<Self, Self::Error> {
        let repo = OwnedRepository::try_from(row.repo)?;
        let grants = parse_grants(row.grant_role.as_deref(), row.grant_base.as_deref(), row.grant_team, row.grant_collaborator.as_deref())?;
        let parent = match (row.parent_id, row.parent_visibility, row.parent_owner, row.parent_name) {
            (Some(id), Some(visibility), Some(owner), Some(name)) => Some(ForkParent {
                id: RepoId(id),
                visibility: Visibility::parse(&visibility).ok_or_else(|| corrupt("parent visibility"))?,
                owner,
                name,
            }),
            (None, ..) => None,
            _ => return Err(corrupt("parent")),
        };
        let mirror = match row.mirror_url {
            Some(url) => Some(Mirror::try_from(MirrorRow {
                repo_id: repo.repo.id.0,
                url,
                interval_minutes: row.mirror_interval_minutes.ok_or_else(|| corrupt("mirror interval"))?,
                next_sync_at: row.mirror_next_sync_at.ok_or_else(|| corrupt("mirror schedule"))?,
                last_sync_at: row.mirror_last_sync_at,
                last_error: row.mirror_last_error,
                created_at: row.mirror_created_at.ok_or_else(|| corrupt("mirror creation"))?,
            })?),
            None => None,
        };
        let count = |n: i64| u64::try_from(n).unwrap_or_default();
        Ok(Self {
            repo,
            grants,
            star_count: count(row.star_count),
            starred_by_viewer: row.starred,
            readable_forks: count(row.fork_count),
            parent,
            mirror,
            open_issues: count(row.open_issues),
            open_pulls: count(row.open_pulls),
        })
    }
}

/// Repositories among the JSON array of ids `?1` that viewer `?2` (site administrator
/// flag `?3`) may read.
///
/// One row per id: the repository and its owner as a JSON array ([`ReadableRepository`]),
/// or `NULL` when it is missing or not readable. The repository is a scalar subquery by
/// primary key, not a join, so it is read by key under any planner statistics (as
/// [`REPOSITORY_PAGE`] explains); joined to the id list (or tested with `IN`), one-row
/// statistics made `SQLite` scan `repositories` and `accounts` instead.
const READABLE_AMONG: &str = concat!(
    "SELECT (SELECT CASE WHEN ",
    readable_by!("?2", "?3"),
    " THEN json_array(r.id, r.owner_id, r.name, r.description, r.visibility, r.default_branch, r.created_at, r.updated_at, \
     r.pushed_at, (SELECT kind FROM accounts WHERE id = r.owner_id), (SELECT name FROM accounts WHERE id = r.owner_id), \
     (SELECT display_name FROM accounts WHERE id = r.owner_id), (SELECT bio FROM accounts WHERE id = r.owner_id), \
     (SELECT created_at FROM accounts WHERE id = r.owner_id)) END \
     FROM repositories r WHERE r.id = ids.value) FROM json_each(?1) ids"
);

/// A row of [`READABLE_AMONG`]: [`OwnedRepositoryRow`]'s columns in order.
type ReadableRepository =
    (i64, i64, String, Option<String>, String, String, i64, i64, Option<i64>, String, String, Option<String>, Option<String>, i64);

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
    pool: Pool,
}

impl RepositoryStore {
    pub(crate) fn new(pool: Pool) -> Self {
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

    /// Repository `owner/name` with everything its pages' header shows, as `viewer` (a site
    /// administrator when `viewer_is_admin`) would see it: one statement instead of the
    /// path lookup, the grants, stars, forks, parent, mirror and open counts one by one.
    /// The caller must check access on [`RepositoryPage::repo`] and
    /// [`RepositoryPage::grants`] before showing anything else.
    pub async fn find_page(
        &self,
        owner: &str,
        name: &str,
        viewer: Option<AccountId>,
        viewer_is_admin: bool,
    ) -> Result<Option<RepositoryPage>, StorageError> {
        let row: Option<RepositoryPageRow> = sqlx::query_as(REPOSITORY_PAGE)
            .bind(owner)
            .bind(name)
            .bind(viewer.map(|id| id.0))
            .bind(viewer_is_admin)
            .fetch_optional(&self.pool)
            .await?;
        row.map(RepositoryPage::try_from).transpose()
    }

    /// The repository formerly at `owner/name`, following account and repository renames
    /// or transfers directly to their stable IDs. A current account shadows its entire
    /// old namespace, and a current repository shadows a repository alias. Both trigger
    /// sets remove reclaimed aliases permanently, including after later renames.
    pub async fn find_by_old_path(&self, owner: &str, name: &str) -> Result<Option<OwnedRepository>, StorageError> {
        let row: Option<OwnedRepositoryRow> = sqlx::query_as(select_owned_repo!(
            "WHERE r.id = (SELECT COALESCE( \
             (SELECT current.id FROM repositories current WHERE current.owner_id = ro.id \
              AND current.name = ?2 AND ro.name <> ?1 COLLATE NOCASE), \
             (SELECT rr.repo_id FROM repo_redirects rr WHERE rr.owner_id = ro.id AND rr.name = ?2)) \
             FROM accounts ro WHERE ro.id = COALESCE((SELECT id FROM accounts WHERE name = ?1), \
             (SELECT account_id FROM account_redirects WHERE name = ?1)))"
        ))
        .bind(owner)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
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

    /// Contribution scans use exactly the same visibility rules as repository pages.
    pub async fn contribution_repositories(&self, viewer: Option<AccountId>, is_admin: bool) -> Result<Vec<OwnedRepository>, StorageError> {
        let rows: Vec<OwnedRepositoryRow> = sqlx::query_as(select_owned_repo!("WHERE ", readable_by!("?1", "?2"), " ORDER BY r.id"))
            .bind(viewer.map(|id| id.0))
            .bind(is_admin)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(OwnedRepository::try_from).collect()
    }

    /// Those of repositories `ids` that `viewer` (a site administrator when
    /// `viewer_is_admin`) may read, by [`readable_by!`], with their owners: one statement
    /// for a page of rows from many repositories (dashboards, the inbox).
    pub async fn readable_among(
        &self,
        ids: &[RepoId],
        viewer: AccountId,
        viewer_is_admin: bool,
    ) -> Result<HashMap<RepoId, OwnedRepository>, StorageError> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let ids = format!("[{}]", ids.iter().map(|id| id.0.to_string()).collect::<Vec<_>>().join(","));
        let rows: Vec<Option<String>> =
            sqlx::query_scalar(READABLE_AMONG).bind(ids).bind(viewer.0).bind(viewer_is_admin).fetch_all(&self.pool).await?;
        rows.into_iter()
            .flatten()
            .map(|json| {
                let row: ReadableRepository =
                    serde_json::from_str(&json).map_err(|e| StorageError::Corrupt(format!("readable repository: {e}")))?;
                let (
                    id,
                    owner_id,
                    name,
                    description,
                    visibility,
                    default_branch,
                    created_at,
                    updated_at,
                    pushed_at,
                    kind,
                    owner,
                    display,
                    bio,
                    since,
                ) = row;
                let repo = OwnedRepository::try_from(OwnedRepositoryRow {
                    repo: RepositoryRow { id, owner_id, name, description, visibility, default_branch, created_at, updated_at, pushed_at },
                    owner_kind: kind,
                    owner_name: owner,
                    owner_display_name: display,
                    owner_bio: bio,
                    owner_created_at: since,
                })?;
                Ok((repo.repo.id, repo))
            })
            .collect()
    }

    /// The dashboard list: repositories owned by `user`, by an organisation they belong
    /// to, or shared with them as a collaborator — each only if they may read it — most
    /// recently updated first. (Team grants are on organisation repositories, so they are
    /// covered by the membership clause.)
    pub async fn list_for_member(&self, user: AccountId, limit: u32) -> Result<Vec<OwnedRepository>, StorageError> {
        self.page_for_member(user, limit, 0).await
    }

    /// One page of [`Self::list_for_member`]'s list, skipping `offset` repositories.
    pub async fn page_for_member(&self, user: AccountId, limit: u32, offset: u32) -> Result<Vec<OwnedRepository>, StorageError> {
        let rows: Vec<OwnedRepositoryRow> =
            sqlx::query_as(select_owned_repo!(member_repositories!(), " ORDER BY r.updated_at DESC, r.id DESC LIMIT ?2 OFFSET ?3"))
                .bind(user.0)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(OwnedRepository::try_from).collect()
    }

    /// How many repositories [`Self::list_for_member`] lists in total.
    pub async fn count_for_member(&self, user: AccountId) -> Result<u64, StorageError> {
        let count: i64 = sqlx::query_scalar(concat!("SELECT COUNT(*) FROM repositories r ", member_repositories!()))
            .bind(user.0)
            .fetch_one(&self.pool)
            .await?;
        Ok(u64::try_from(count).unwrap_or_default())
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
        parse_grants(role.as_deref(), base.as_deref(), team, collaborator.as_deref())
    }

    /// Fresh grants for all requested users, deduplicated and queried in bounded batches.
    /// Users with no grants still have an entry, with the owner's default base permission.
    pub async fn grants_for(
        &self,
        repo: RepoId,
        owner: AccountId,
        users: &[AccountId],
    ) -> Result<HashMap<AccountId, ViewerGrants>, StorageError> {
        let mut ids: Vec<i64> = users.iter().map(|id| id.0).collect();
        ids.sort_unstable();
        ids.dedup();
        let mut grants = HashMap::with_capacity(ids.len());
        for batch in ids.chunks(GRANT_QUERY_BATCH_SIZE) {
            let rows: Vec<GrantsRow> = sqlx::query_as(
                "SELECT v.value AS user_id, \
                 (SELECT role FROM org_members WHERE org_id = ?1 AND user_id = v.value) AS role, \
                 (SELECT member_permission FROM org_profiles WHERE account_id = ?1) AS base, \
                 (SELECT MAX(CASE tr.permission WHEN 'admin' THEN 3 WHEN 'write' THEN 2 ELSE 1 END) \
                  FROM team_repositories tr JOIN team_members tm ON tm.team_id = tr.team_id JOIN teams t ON t.id = tr.team_id \
                  WHERE tr.repo_id = ?2 AND t.org_id = ?1 AND tm.user_id = v.value) AS team, \
                 (SELECT permission FROM collaborators WHERE repo_id = ?2 AND user_id = v.value) AS collaborator \
                 FROM json_each(?3) v",
            )
            .bind(owner.0)
            .bind(repo.0)
            .bind(format!("{batch:?}"))
            .fetch_all(&self.pool)
            .await?;
            for row in rows {
                grants.insert(
                    AccountId(row.user_id),
                    parse_grants(row.role.as_deref(), row.base.as_deref(), row.team, row.collaborator.as_deref())?,
                );
            }
        }
        Ok(grants)
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
    use super::{READABLE_AMONG, REPOSITORY_PAGE, contains_pattern};
    use crate::sqlite::plans;

    /// The folded page lookup reads each table through the index its separate query used:
    /// the path through the owner's name and `(owner_id, name)`, stars through
    /// `stars_by_repo` and the primary key, forks through `repo_forks_by_parent`, and the
    /// open counts through the open entries of `issues_by_repo`.
    #[tokio::test]
    async fn repository_page_lookup_uses_the_indexes() {
        let (_dir, database) = plans::database().await;
        let plan = plans::query_plan(&database, REPOSITORY_PAGE).await;
        let has = |line: &str| plan.iter().any(|detail| detail == line);
        assert!(!plan.iter().any(|detail| detail.starts_with("SCAN")), "no table is scanned: {plan:#?}");
        for line in [
            // The path, then the one-row lookups by primary key.
            "SEARCH accounts USING COVERING INDEX sqlite_autoindex_accounts_1 (name=?)",
            "SEARCH page USING INDEX sqlite_autoindex_repositories_1 (owner_id=? AND name=?)",
            "SEARCH accounts USING INTEGER PRIMARY KEY (rowid=?)",
            "SEARCH repo_forks USING INTEGER PRIMARY KEY (rowid=?)",
            "SEARCH repositories USING INTEGER PRIMARY KEY (rowid=?)",
            "SEARCH repo_mirrors USING INTEGER PRIMARY KEY (rowid=?)",
            "SEARCH r USING INTEGER PRIMARY KEY (rowid=?)",
            // The grants, as `grants` reads them.
            "SEARCH org_members USING PRIMARY KEY (org_id=? AND user_id=?)",
            "SEARCH collaborators USING PRIMARY KEY (repo_id=? AND user_id=?)",
            // Stars: the count from `stars_by_repo`, the viewer's star by primary key.
            "SEARCH stars USING COVERING INDEX stars_by_repo (repo_id=?)",
            "SEARCH stars USING PRIMARY KEY (account_id=? AND repo_id=?)",
            // Forks from their parent index.
            "SEARCH f USING COVERING INDEX repo_forks_by_parent (parent_id=?)",
        ] {
            assert!(has(line), "{line}: {plan:#?}");
        }
        // Both open counts read only open entries, as `OPEN_COUNTS` does.
        let open = plan.iter().filter(|detail| {
            detail.starts_with("SEARCH issues USING COVERING INDEX ") && detail.ends_with(" (repo_id=? AND kind=? AND state=?)")
        });
        assert_eq!(open.count(), 2, "{plan:#?}");
    }

    /// The batch looks each repository up by key; the access checks are by key too.
    #[tokio::test]
    async fn readable_among_reads_by_key() {
        let (_dir, database) = plans::database().await;
        let plan = plans::query_plan(&database, READABLE_AMONG).await;
        assert!(plan.iter().any(|detail| detail == "SEARCH r USING INTEGER PRIMARY KEY (rowid=?)"), "{plan:#?}");
        let scans: Vec<_> = plan.iter().filter(|detail| detail.starts_with("SCAN")).collect();
        assert!(scans.iter().all(|detail| detail.starts_with("SCAN ids ")), "only the id list is scanned: {plan:#?}");
        plans::assert_no_scans(&database, READABLE_AMONG, &["SCAN ids "]).await;
    }

    /// Under any statistics, as the planner flips plans with table sizes.
    #[tokio::test]
    async fn repository_page_lookup_never_scans() {
        let (_dir, database) = plans::database().await;
        plans::assert_no_scans(&database, REPOSITORY_PAGE, &[]).await;
    }

    #[test]
    fn like_wildcards_are_escaped() {
        assert_eq!(contains_pattern(""), "%%");
        assert_eq!(contains_pattern("web"), "%web%");
        assert_eq!(contains_pattern(r"100%_a\b"), r"%100\%\_a\\b%");
    }
}
