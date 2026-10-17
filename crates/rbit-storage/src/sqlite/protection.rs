//! Branch protection rules with their allowed users and teams.
//!
//! Milestone 2 workstream B (branch protection) owns this file. Keep row types here rather than in `rows.rs`.
//!
//! Invariants enforced here, in the same transaction as the change: at most
//! [`MAX_RULES`] rules per repository, patterns unique per repository (case-sensitively),
//! and allowed teams belong to the organisation that owns the repository. Validation of
//! the values themselves (patterns, approval counts, list lengths, write access of
//! allowed users) happens in the service.

use std::collections::HashMap;

use rbit_core::{
    Account, AccountId, RepoId, Timestamp,
    membership::{Team, TeamId},
    protection::{BranchPattern, BranchProtection, MAX_RULES, ProtectionId, ProtectionRule, RuleSettings},
};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::rows::AccountRow;
use crate::StorageError;

/// The columns of a rule row, in [`RuleRow`]'s order.
macro_rules! rule_columns {
    () => {
        "id, repo_id, pattern, deny_force_push, deny_deletion, require_pull_request, required_approvals, \
         dismiss_stale_approvals, block_on_changes_requested, restrict_pushers, allow_admin_bypass, created_by, created_at, updated_at"
    };
}

#[derive(Debug, sqlx::FromRow)]
#[expect(clippy::struct_excessive_bools, reason = "one column per rule flag")]
struct RuleRow {
    id: i64,
    repo_id: i64,
    pattern: String,
    deny_force_push: bool,
    deny_deletion: bool,
    require_pull_request: bool,
    required_approvals: i64,
    dismiss_stale_approvals: bool,
    block_on_changes_requested: bool,
    restrict_pushers: bool,
    allow_admin_bypass: bool,
    created_by: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct UserGrantRow {
    rule_id: i64,
    #[sqlx(flatten)]
    account: AccountRow,
}

#[derive(Debug, sqlx::FromRow)]
struct TeamGrantRow {
    rule_id: i64,
    id: i64,
    org_id: i64,
    name: String,
    description: Option<String>,
    created_at: i64,
}

/// A rule as written: everything but its id and timestamps.
#[derive(Debug, Clone)]
pub struct RuleInput {
    pub pattern: BranchPattern,
    pub settings: RuleSettings,
    pub users: Vec<AccountId>,
    pub teams: Vec<TeamId>,
}

/// The result of [`BranchProtectionStore::create`] and [`BranchProtectionStore::update`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleChange {
    Saved(ProtectionId),
    /// Creating: the repository already has [`MAX_RULES`] rules.
    TooManyRules,
    /// Updating: there is no such rule in the repository.
    NotFound,
    /// An allowed team does not belong to the repository's organisation (nothing was saved).
    TeamOutsideOrganization,
}

#[derive(Debug, Clone)]
pub struct BranchProtectionStore {
    pool: SqlitePool,
}

impl BranchProtectionStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The rules of `repo`, by pattern, with their allowed users and teams (by name).
    /// Three queries whatever the number of rules.
    pub async fn list(&self, repo: RepoId) -> Result<Vec<BranchProtection>, StorageError> {
        let rows: Vec<RuleRow> =
            sqlx::query_as(concat!("SELECT ", rule_columns!(), " FROM branch_protections WHERE repo_id = ? ORDER BY pattern"))
                .bind(repo.0)
                .fetch_all(&self.pool)
                .await?;
        let users: Vec<UserGrantRow> = sqlx::query_as(
            "SELECT g.rule_id, a.id, a.kind, a.name, a.display_name, a.bio, a.created_at \
             FROM branch_protection_users g JOIN branch_protections p ON p.id = g.rule_id JOIN accounts a ON a.id = g.user_id \
             WHERE p.repo_id = ? ORDER BY a.name COLLATE NOCASE",
        )
        .bind(repo.0)
        .fetch_all(&self.pool)
        .await?;
        let teams: Vec<TeamGrantRow> = sqlx::query_as(
            "SELECT g.rule_id, t.id, t.org_id, t.name, t.description, t.created_at \
             FROM branch_protection_teams g JOIN branch_protections p ON p.id = g.rule_id JOIN teams t ON t.id = g.team_id \
             WHERE p.repo_id = ? ORDER BY t.name COLLATE NOCASE",
        )
        .bind(repo.0)
        .fetch_all(&self.pool)
        .await?;

        let mut users_by_rule: HashMap<i64, Vec<Account>> = HashMap::new();
        for row in users {
            users_by_rule.entry(row.rule_id).or_default().push(row.account.try_into()?);
        }
        let mut teams_by_rule: HashMap<i64, Vec<Team>> = HashMap::new();
        for row in teams {
            let team = Team {
                id: TeamId(row.id),
                org_id: AccountId(row.org_id),
                name: row.name.parse().map_err(|e| StorageError::Corrupt(format!("team name: {e}")))?,
                description: row.description,
                created_at: Timestamp(row.created_at),
            };
            teams_by_rule.entry(row.rule_id).or_default().push(team);
        }
        rows.into_iter()
            .map(|row| {
                let users = users_by_rule.remove(&row.id).unwrap_or_default();
                let teams = teams_by_rule.remove(&row.id).unwrap_or_default();
                protection(&row, users, teams)
            })
            .collect()
    }

    /// Rule `id` of `repo`.
    pub async fn get(&self, repo: RepoId, id: ProtectionId) -> Result<Option<BranchProtection>, StorageError> {
        Ok(self.list(repo).await?.into_iter().find(|rule| rule.id == id))
    }

    /// The teams of `repo`'s organisation that `user` is on (none for personal repositories).
    pub async fn teams_of(&self, repo: RepoId, user: AccountId) -> Result<Vec<TeamId>, StorageError> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT m.team_id FROM team_members m JOIN teams t ON t.id = m.team_id JOIN repositories r ON r.owner_id = t.org_id \
             WHERE r.id = ? AND m.user_id = ? ORDER BY m.team_id",
        )
        .bind(repo.0)
        .bind(user.0)
        .fetch_all(&self.pool)
        .await?;
        Ok(ids.into_iter().map(TeamId).collect())
    }

    /// Adds a rule to `repo`. [`StorageError::Conflict`] (`"pattern"`) if `repo` already has
    /// a rule for this pattern.
    pub async fn create(&self, repo: RepoId, input: &RuleInput, creator: AccountId) -> Result<RuleChange, StorageError> {
        let now = Timestamp::now().seconds();
        let mut tx = self.pool.begin().await?;
        let settings = &input.settings;
        let id: Option<i64> = sqlx::query_scalar(
            "INSERT INTO branch_protections (repo_id, pattern, deny_force_push, deny_deletion, require_pull_request, \
             required_approvals, dismiss_stale_approvals, block_on_changes_requested, restrict_pushers, allow_admin_bypass, \
             created_by, created_at, updated_at) \
             SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12 \
             WHERE (SELECT COUNT(*) FROM branch_protections WHERE repo_id = ?1) < ?13 RETURNING id",
        )
        .bind(repo.0)
        .bind(input.pattern.as_str())
        .bind(settings.deny_force_push)
        .bind(settings.deny_deletion)
        .bind(settings.require_pull_request)
        .bind(i64::from(settings.required_approvals))
        .bind(settings.dismiss_stale_approvals)
        .bind(settings.block_on_changes_requested)
        .bind(settings.restrict_pushers)
        .bind(settings.allow_admin_bypass)
        .bind(creator.0)
        .bind(now)
        .bind(i64::try_from(MAX_RULES).unwrap_or(i64::MAX))
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| StorageError::from_insert(e, "pattern"))?;
        let Some(id) = id else {
            return Ok(RuleChange::TooManyRules);
        };
        if !insert_grants(&mut tx, repo, id, input).await? {
            return Ok(RuleChange::TeamOutsideOrganization);
        }
        tx.commit().await?;
        Ok(RuleChange::Saved(ProtectionId(id)))
    }

    /// Replaces rule `id` of `repo` (pattern, flags, users and teams).
    /// [`StorageError::Conflict`] (`"pattern"`) if another rule has the new pattern.
    pub async fn update(&self, repo: RepoId, id: ProtectionId, input: &RuleInput) -> Result<RuleChange, StorageError> {
        let mut tx = self.pool.begin().await?;
        let settings = &input.settings;
        let result = sqlx::query(
            "UPDATE branch_protections SET pattern = ?, deny_force_push = ?, deny_deletion = ?, require_pull_request = ?, \
             required_approvals = ?, dismiss_stale_approvals = ?, block_on_changes_requested = ?, restrict_pushers = ?, \
             allow_admin_bypass = ?, updated_at = ? WHERE id = ? AND repo_id = ?",
        )
        .bind(input.pattern.as_str())
        .bind(settings.deny_force_push)
        .bind(settings.deny_deletion)
        .bind(settings.require_pull_request)
        .bind(i64::from(settings.required_approvals))
        .bind(settings.dismiss_stale_approvals)
        .bind(settings.block_on_changes_requested)
        .bind(settings.restrict_pushers)
        .bind(settings.allow_admin_bypass)
        .bind(Timestamp::now().seconds())
        .bind(id.0)
        .bind(repo.0)
        .execute(&mut *tx)
        .await
        .map_err(|e| StorageError::from_insert(e, "pattern"))?;
        if result.rows_affected() == 0 {
            return Ok(RuleChange::NotFound);
        }
        sqlx::query("DELETE FROM branch_protection_users WHERE rule_id = ?").bind(id.0).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM branch_protection_teams WHERE rule_id = ?").bind(id.0).execute(&mut *tx).await?;
        if !insert_grants(&mut tx, repo, id.0, input).await? {
            return Ok(RuleChange::TeamOutsideOrganization);
        }
        tx.commit().await?;
        Ok(RuleChange::Saved(id))
    }

    /// Deletes rule `id` of `repo`. Returns whether it existed.
    pub async fn delete(&self, repo: RepoId, id: ProtectionId) -> Result<bool, StorageError> {
        let result =
            sqlx::query("DELETE FROM branch_protections WHERE id = ? AND repo_id = ?").bind(id.0).bind(repo.0).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }
}

/// Writes the allowed users and teams of rule `id`. `false` (and nothing more written) if
/// a team does not belong to the organisation owning `repo`; the caller then drops the
/// transaction.
async fn insert_grants(tx: &mut Transaction<'_, Sqlite>, repo: RepoId, id: i64, input: &RuleInput) -> Result<bool, StorageError> {
    for user in &input.users {
        sqlx::query("INSERT OR IGNORE INTO branch_protection_users (rule_id, user_id) VALUES (?, ?)")
            .bind(id)
            .bind(user.0)
            .execute(&mut **tx)
            .await?;
    }
    for team in &input.teams {
        let result = sqlx::query(
            "INSERT OR IGNORE INTO branch_protection_teams (rule_id, team_id) SELECT ?1, t.id FROM teams t \
             JOIN repositories r ON r.owner_id = t.org_id WHERE t.id = ?2 AND r.id = ?3",
        )
        .bind(id)
        .bind(team.0)
        .bind(repo.0)
        .execute(&mut **tx)
        .await?;
        if result.rows_affected() == 0 {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM branch_protection_teams WHERE rule_id = ? AND team_id = ?)")
                    .bind(id)
                    .bind(team.0)
                    .fetch_one(&mut **tx)
                    .await?;
            if !exists {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn protection(row: &RuleRow, users: Vec<Account>, teams: Vec<Team>) -> Result<BranchProtection, StorageError> {
    let corrupt = |what: &str, detail: String| StorageError::Corrupt(format!("branch protection {what}: {detail}"));
    let pattern = BranchPattern::parse(&row.pattern).map_err(|e| corrupt("pattern", e.to_string()))?;
    let settings = RuleSettings {
        deny_force_push: row.deny_force_push,
        deny_deletion: row.deny_deletion,
        require_pull_request: row.require_pull_request,
        required_approvals: u8::try_from(row.required_approvals).map_err(|e| corrupt("approvals", e.to_string()))?,
        dismiss_stale_approvals: row.dismiss_stale_approvals,
        block_on_changes_requested: row.block_on_changes_requested,
        restrict_pushers: row.restrict_pushers,
        allow_admin_bypass: row.allow_admin_bypass,
    };
    let rule = ProtectionRule {
        pattern,
        settings,
        allowed_users: users.iter().map(|account| account.id).collect(),
        allowed_teams: teams.iter().map(|team| team.id).collect(),
    };
    Ok(BranchProtection {
        id: ProtectionId(row.id),
        repo_id: RepoId(row.repo_id),
        rule,
        users,
        teams,
        created_by: row.created_by.map(AccountId),
        created_at: Timestamp(row.created_at),
        updated_at: Timestamp(row.updated_at),
    })
}
