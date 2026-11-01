//! Organisation teams, their members and their repository permissions.
//!
//! Milestone 2 workstream A (access) owns this file. Keep row types here rather than in `rows.rs`.
//!
//! Invariants enforced here (in the same statement, so concurrent requests cannot break
//! them): team members are members of the team's organisation (and a trigger removes
//! them from its teams when they leave it), and teams are only granted repositories of
//! their own organisation.

use super::pool::Pool;
use rbit_core::{
    Account, AccountId, RepoId, Timestamp,
    membership::{Permission, Team, TeamGrant, TeamId, TeamName, TeamSummary},
    repository::OwnedRepository,
};

use super::{
    collaborators::parse_permission,
    repositories::readable_by,
    rows::{AccountRow, OwnedRepositoryRow},
};
use crate::StorageError;

#[derive(Debug, sqlx::FromRow)]
struct TeamRow {
    id: i64,
    org_id: i64,
    name: String,
    description: Option<String>,
    created_at: i64,
}

impl TryFrom<TeamRow> for Team {
    type Error = StorageError;

    fn try_from(row: TeamRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: TeamId(row.id),
            org_id: AccountId(row.org_id),
            name: row.name.parse().map_err(|e| StorageError::Corrupt(format!("team name: {e}")))?,
            description: row.description,
            created_at: Timestamp(row.created_at),
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct TeamSummaryRow {
    #[sqlx(flatten)]
    team: TeamRow,
    member_count: i64,
    repo_count: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct TeamGrantRow {
    #[sqlx(flatten)]
    team: TeamRow,
    permission: String,
}

#[derive(Debug, sqlx::FromRow)]
struct TeamRepositoryRow {
    #[sqlx(flatten)]
    repo: OwnedRepositoryRow,
    permission: String,
}

/// Result of a change that needs a precondition checked in the same statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamChange {
    Done,
    /// Adding a member: the user is not a member of the team's organisation.
    /// Granting a repository: it does not belong to the team's organisation.
    OutsideOrganization,
}

#[derive(Debug, Clone)]
pub struct TeamStore {
    pool: Pool,
    writer: rbit_authority_fence::WriterRegistration,
}

impl TeamStore {
    pub(crate) fn new(pool: Pool, writer: rbit_authority_fence::WriterRegistration) -> Self {
        Self { pool, writer }
    }

    /// [`StorageError::Conflict`] (`"team name"`) if the organisation already has a team
    /// with this name (case-insensitively).
    pub async fn create(&self, org: AccountId, name: &TeamName, description: Option<&str>) -> Result<Team, StorageError> {
        let mutation = self.writer.begin().await.map_err(team_mutation_error)?;
        let result = self.create_raw(org, name, description).await;
        if result.is_ok() {
            mutation.complete();
        }
        result
    }

    async fn create_raw(&self, org: AccountId, name: &TeamName, description: Option<&str>) -> Result<Team, StorageError> {
        let mut tx = self.pool.begin().await?;
        let row: TeamRow = sqlx::query_as(
            "INSERT INTO teams (org_id, name, description, created_at) VALUES (?, ?, ?, ?) \
             RETURNING id, org_id, name, description, created_at",
        )
        .bind(org.0)
        .bind(name.as_str())
        .bind(description)
        .bind(Timestamp::now().seconds())
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| StorageError::from_insert(e, "team name"))?;
        tx.commit().await?;
        row.try_into()
    }

    /// The team of `org` called `name` (case-insensitively).
    pub async fn find_by_name(&self, org: AccountId, name: &str) -> Result<Option<Team>, StorageError> {
        let row: Option<TeamRow> =
            sqlx::query_as("SELECT id, org_id, name, description, created_at FROM teams WHERE org_id = ? AND name = ?")
                .bind(org.0)
                .bind(name)
                .fetch_optional(&self.pool)
                .await?;
        row.map(Team::try_from).transpose()
    }

    /// Every team of `org` with its member and repository counts, by name.
    pub async fn list(&self, org: AccountId) -> Result<Vec<TeamSummary>, StorageError> {
        let rows: Vec<TeamSummaryRow> = sqlx::query_as(
            "SELECT t.id, t.org_id, t.name, t.description, t.created_at, \
             (SELECT COUNT(*) FROM team_members m WHERE m.team_id = t.id) AS member_count, \
             (SELECT COUNT(*) FROM team_repositories r WHERE r.team_id = t.id) AS repo_count \
             FROM teams t WHERE t.org_id = ? ORDER BY t.name COLLATE NOCASE",
        )
        .bind(org.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| Ok(TeamSummary { team: row.team.try_into()?, member_count: row.member_count, repo_count: row.repo_count }))
            .collect()
    }

    /// Renames the team and replaces its description. [`StorageError::Conflict`] on a name clash.
    pub async fn update(&self, team: TeamId, name: &TeamName, description: Option<&str>) -> Result<(), StorageError> {
        let mutation = self.writer.begin().await.map_err(team_mutation_error)?;
        let result = self.update_raw(team, name, description).await;
        if result.is_ok() {
            mutation.complete();
        }
        result
    }

    async fn update_raw(&self, team: TeamId, name: &TeamName, description: Option<&str>) -> Result<(), StorageError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE teams SET name = ?, description = ? WHERE id = ?")
            .bind(name.as_str())
            .bind(description)
            .bind(team.0)
            .execute(&mut *tx)
            .await
            .map_err(|e| StorageError::from_insert(e, "team name"))?;
        tx.commit().await?;
        Ok(())
    }

    /// Deletes the team with its memberships and repository grants.
    pub async fn delete(&self, team: TeamId) -> Result<(), StorageError> {
        let mutation = self.writer.begin().await.map_err(team_mutation_error)?;
        let result = self.delete_raw(team).await;
        if result.is_ok() {
            mutation.complete();
        }
        result
    }

    async fn delete_raw(&self, team: TeamId) -> Result<(), StorageError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM teams WHERE id = ?").bind(team.0).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Members of `team`, by name.
    pub async fn members(&self, team: TeamId) -> Result<Vec<Account>, StorageError> {
        let rows: Vec<AccountRow> = sqlx::query_as(
            "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at \
             FROM team_members m JOIN accounts a ON a.id = m.user_id WHERE m.team_id = ? ORDER BY a.name COLLATE NOCASE",
        )
        .bind(team.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Account::try_from).collect()
    }

    pub async fn is_member(&self, team: TeamId, user: AccountId) -> Result<bool, StorageError> {
        Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM team_members WHERE team_id = ? AND user_id = ?)")
            .bind(team.0)
            .bind(user.0)
            .fetch_one(&self.pool)
            .await?)
    }

    /// Adds `user` to `team` if they belong to the team's organisation.
    /// [`StorageError::Conflict`] (`"team member"`) if they already are on the team.
    pub async fn add_member(&self, team: TeamId, user: AccountId) -> Result<TeamChange, StorageError> {
        let mutation = self.writer.begin().await.map_err(team_mutation_error)?;
        let result = self.add_member_raw(team, user).await;
        if result.is_ok() {
            mutation.complete();
        }
        result
    }

    async fn add_member_raw(&self, team: TeamId, user: AccountId) -> Result<TeamChange, StorageError> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            "INSERT INTO team_members (team_id, user_id) SELECT ?1, ?2 \
             WHERE EXISTS (SELECT 1 FROM teams t JOIN org_members m ON m.org_id = t.org_id WHERE t.id = ?1 AND m.user_id = ?2)",
        )
        .bind(team.0)
        .bind(user.0)
        .execute(&mut *tx)
        .await
        .map_err(|e| StorageError::from_insert(e, "team member"))?;
        tx.commit().await?;
        Ok(if result.rows_affected() > 0 { TeamChange::Done } else { TeamChange::OutsideOrganization })
    }

    /// Removes `user` from `team`. Returns whether they were on it.
    pub async fn remove_member(&self, team: TeamId, user: AccountId) -> Result<bool, StorageError> {
        let mutation = self.writer.begin().await.map_err(team_mutation_error)?;
        let result = self.remove_member_raw(team, user).await;
        if result.is_ok() {
            mutation.complete();
        }
        result
    }

    async fn remove_member_raw(&self, team: TeamId, user: AccountId) -> Result<bool, StorageError> {
        let mut tx = self.pool.begin().await?;
        let result =
            sqlx::query("DELETE FROM team_members WHERE team_id = ? AND user_id = ?").bind(team.0).bind(user.0).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    /// Grants `team` `permission` on `repo` (or changes it), if the repository belongs to
    /// the team's organisation.
    pub async fn set_repository(&self, team: TeamId, repo: RepoId, permission: Permission) -> Result<TeamChange, StorageError> {
        let mutation = self.writer.begin().await.map_err(team_mutation_error)?;
        let result = self.set_repository_raw(team, repo, permission).await;
        if result.is_ok() {
            mutation.complete();
        }
        result
    }

    async fn set_repository_raw(&self, team: TeamId, repo: RepoId, permission: Permission) -> Result<TeamChange, StorageError> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            "INSERT INTO team_repositories (team_id, repo_id, permission) SELECT ?1, ?2, ?3 \
             WHERE EXISTS (SELECT 1 FROM teams t JOIN repositories r ON r.owner_id = t.org_id WHERE t.id = ?1 AND r.id = ?2) \
             ON CONFLICT (team_id, repo_id) DO UPDATE SET permission = ?3",
        )
        .bind(team.0)
        .bind(repo.0)
        .bind(permission.as_str())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(if result.rows_affected() > 0 { TeamChange::Done } else { TeamChange::OutsideOrganization })
    }

    /// Revokes `team`'s access to `repo`. Returns whether it had any.
    pub async fn remove_repository(&self, team: TeamId, repo: RepoId) -> Result<bool, StorageError> {
        let mutation = self.writer.begin().await.map_err(team_mutation_error)?;
        let result = self.remove_repository_raw(team, repo).await;
        if result.is_ok() {
            mutation.complete();
        }
        result
    }

    async fn remove_repository_raw(&self, team: TeamId, repo: RepoId) -> Result<bool, StorageError> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query("DELETE FROM team_repositories WHERE team_id = ? AND repo_id = ?")
            .bind(team.0)
            .bind(repo.0)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    /// The repositories `team` was granted that `viewer` may read, by name, with the
    /// team's permission on each. (Members of the team can read all of them; other
    /// organisation members only see those they could open anyway.)
    pub async fn repositories(
        &self,
        team: TeamId,
        viewer: Option<AccountId>,
        viewer_is_admin: bool,
    ) -> Result<Vec<(OwnedRepository, Permission)>, StorageError> {
        let rows: Vec<TeamRepositoryRow> = sqlx::query_as(concat!(
            "SELECT r.id, r.owner_id, r.name, r.description, r.visibility, r.default_branch, r.created_at, r.updated_at, \
             r.pushed_at, (SELECT a.archived_at FROM repo_archives a WHERE a.repo_id = r.id) AS archived_at, \
             o.kind AS owner_kind, o.name AS owner_name, o.display_name AS owner_display_name, \
             o.bio AS owner_bio, o.created_at AS owner_created_at, g.permission \
             FROM team_repositories g JOIN repositories r ON r.id = g.repo_id JOIN accounts o ON o.id = r.owner_id \
             WHERE g.team_id = ?1 AND ",
            readable_by!("?2", "?3"),
            " ORDER BY r.name COLLATE NOCASE"
        ))
        .bind(team.0)
        .bind(viewer.map(|id| id.0))
        .bind(viewer_is_admin)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(|row| Ok((OwnedRepository::try_from(row.repo)?, parse_permission(&row.permission)?))).collect()
    }

    /// The teams granted access to `repo`, by name.
    pub async fn grants_for_repository(&self, repo: RepoId) -> Result<Vec<TeamGrant>, StorageError> {
        let rows: Vec<TeamGrantRow> = sqlx::query_as(
            "SELECT t.id, t.org_id, t.name, t.description, t.created_at, g.permission \
             FROM team_repositories g JOIN teams t ON t.id = g.team_id WHERE g.repo_id = ? ORDER BY t.name COLLATE NOCASE",
        )
        .bind(repo.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(|row| Ok(TeamGrant { team: row.team.try_into()?, permission: parse_permission(&row.permission)? })).collect()
    }
}

// All seven direct TeamStore writer APIs use this registration. Other
// transfer/cascade origins and service freshness remain separate categories.
fn team_mutation_error(error: rbit_authority_fence::BeginError) -> StorageError {
    StorageError::AdmissionUnavailable {
        scope: "team",
        reason: match error {
            rbit_authority_fence::BeginError::Capacity => crate::AdmissionReason::Capacity,
            rbit_authority_fence::BeginError::Stopped => crate::AdmissionReason::Stopped,
        },
    }
}

#[cfg(test)]
#[path = "teams/fence_tests.rs"]
mod fence_tests;

#[cfg(test)]
#[path = "teams/lifecycle_fence_tests.rs"]
mod lifecycle_fence_tests;
