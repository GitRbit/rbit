//! Actions settings (migration `0048_actions.sql`): the per-repository switch and default
//! token permission, which owners instance runners serve, and the pull request authors a
//! repository trusts; and (migration `0054`) the policies of GitHub's
//! `actions/permissions` endpoints, a repository's and its owner's, with the repositories
//! an owner selected to run workflows.
//!
//! The Actions foundation (workstream F) owns this file.

use rbit_core::{
    AccountId, RepoId, Timestamp,
    actions::{
        DefaultTokenPermission, RepoActionsSettings, TrustedUser,
        permissions::{ActionsPolicies, AllowedActions, ApprovalPolicy, EnabledRepositories, OwnerActionsSettings, SelectedActions},
    },
};
use sqlx::SqlitePool;

use super::actions_rows::corrupt;
use crate::StorageError;

#[derive(Debug, sqlx::FromRow)]
struct SettingsRow {
    repo_id: i64,
    enabled: bool,
    token_permission: String,
    updated_at: i64,
}

impl TryFrom<SettingsRow> for RepoActionsSettings {
    type Error = StorageError;

    fn try_from(row: SettingsRow) -> Result<Self, Self::Error> {
        Ok(Self {
            repo_id: RepoId(row.repo_id),
            enabled: row.enabled,
            token_permission: DefaultTokenPermission::parse(&row.token_permission)
                .ok_or_else(|| corrupt("token permission", &row.token_permission))?,
            updated_at: Timestamp(row.updated_at),
        })
    }
}

// Actions: settings API.
#[derive(Debug, sqlx::FromRow)]
struct PoliciesRow {
    allowed_actions: String,
    github_owned_allowed: bool,
    patterns_allowed: String,
    token_permission: String,
    can_approve_reviews: bool,
    approval_policy: String,
}

#[derive(Debug, sqlx::FromRow)]
struct OwnerRow {
    enabled_repositories: String,
    allowed_actions: String,
    github_owned_allowed: bool,
    patterns_allowed: String,
    token_permission: String,
    can_approve_reviews: bool,
    approval_policy: String,
}

impl TryFrom<PoliciesRow> for ActionsPolicies {
    type Error = StorageError;

    fn try_from(row: PoliciesRow) -> Result<Self, Self::Error> {
        let patterns_allowed: Vec<String> =
            serde_json::from_str(&row.patterns_allowed).map_err(|_| corrupt("allowed action patterns", &row.patterns_allowed))?;
        Ok(Self {
            allowed_actions: AllowedActions::parse(&row.allowed_actions).ok_or_else(|| corrupt("allowed actions", &row.allowed_actions))?,
            selected_actions: SelectedActions { github_owned_allowed: row.github_owned_allowed, patterns_allowed },
            token_permission: DefaultTokenPermission::parse(&row.token_permission)
                .ok_or_else(|| corrupt("token permission", &row.token_permission))?,
            can_approve_reviews: row.can_approve_reviews,
            approval_policy: ApprovalPolicy::parse(&row.approval_policy).ok_or_else(|| corrupt("approval policy", &row.approval_policy))?,
        })
    }
}

fn patterns_json(patterns: &[String]) -> String {
    serde_json::to_string(patterns).unwrap_or_else(|_| "[]".to_owned())
}

#[derive(Debug, Clone)]
pub struct ActionsSettingsStore {
    pool: SqlitePool,
}

impl ActionsSettingsStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The repository's settings; `None` when none were ever stored (Actions is off).
    pub async fn get(&self, repo: RepoId) -> Result<Option<RepoActionsSettings>, StorageError> {
        let row: Option<SettingsRow> =
            sqlx::query_as("SELECT repo_id, enabled, token_permission, updated_at FROM actions_repositories WHERE repo_id = ?")
                .bind(repo.0)
                .fetch_optional(&self.pool)
                .await?;
        row.map(RepoActionsSettings::try_from).transpose()
    }

    /// Whether workflows run in the repository: it turned Actions on, and its owner lets
    /// it (every repository, or the selected ones with it among them).
    pub async fn is_enabled(&self, repo: RepoId) -> Result<bool, StorageError> {
        let enabled: Option<bool> = sqlx::query_scalar(
            "SELECT a.enabled = 1 AND CASE COALESCE(o.enabled_repositories, 'all') \
                 WHEN 'all' THEN 1 \
                 WHEN 'selected' THEN EXISTS (SELECT 1 FROM actions_owner_enabled_repositories e \
                                              WHERE e.owner_id = r.owner_id AND e.repo_id = r.id) \
                 ELSE 0 END \
             FROM actions_repositories a JOIN repositories r ON r.id = a.repo_id \
             LEFT JOIN actions_owner_settings o ON o.owner_id = r.owner_id WHERE a.repo_id = ?",
        )
        .bind(repo.0)
        .fetch_optional(&self.pool)
        .await?;
        Ok(enabled.unwrap_or(false))
    }

    // Actions: settings API.
    /// Whether the repository itself turned Actions on, whatever its owner allows.
    pub async fn is_switched_on(&self, repo: RepoId) -> Result<bool, StorageError> {
        let enabled: Option<bool> = sqlx::query_scalar("SELECT enabled FROM actions_repositories WHERE repo_id = ?")
            .bind(repo.0)
            .fetch_optional(&self.pool)
            .await?;
        Ok(enabled.unwrap_or(false))
    }

    /// The repository's own policies; [`ActionsPolicies::repository_default`] when it never
    /// stored any.
    pub async fn policies(&self, repo: RepoId) -> Result<ActionsPolicies, StorageError> {
        let row: Option<PoliciesRow> = sqlx::query_as(
            "SELECT allowed_actions, github_owned_allowed, patterns_allowed, token_permission, can_approve_reviews, approval_policy \
             FROM actions_repositories WHERE repo_id = ?",
        )
        .bind(repo.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map_or_else(|| Ok(ActionsPolicies::repository_default()), ActionsPolicies::try_from)
    }

    /// Stores the repository's policies, its switch untouched (off when it had no row).
    pub async fn set_policies(&self, repo: RepoId, policies: &ActionsPolicies, now: Timestamp) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO actions_repositories (repo_id, allowed_actions, github_owned_allowed, patterns_allowed, token_permission, \
             can_approve_reviews, approval_policy, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT (repo_id) DO UPDATE SET allowed_actions = ?2, github_owned_allowed = ?3, patterns_allowed = ?4, \
             token_permission = ?5, can_approve_reviews = ?6, approval_policy = ?7, updated_at = ?8",
        )
        .bind(repo.0)
        .bind(policies.allowed_actions.as_str())
        .bind(policies.selected_actions.github_owned_allowed)
        .bind(patterns_json(&policies.selected_actions.patterns_allowed))
        .bind(policies.token_permission.as_str())
        .bind(policies.can_approve_reviews)
        .bind(policies.approval_policy.as_str())
        .bind(now.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The owner's settings; [`OwnerActionsSettings::default`] (no ceiling) when it never
    /// stored any.
    pub async fn owner_settings(&self, owner: AccountId) -> Result<OwnerActionsSettings, StorageError> {
        let row: Option<(String, PoliciesRow)> = sqlx::query_as::<_, OwnerRow>(
            "SELECT enabled_repositories, allowed_actions, github_owned_allowed, patterns_allowed, token_permission, \
             can_approve_reviews, approval_policy FROM actions_owner_settings WHERE owner_id = ?",
        )
        .bind(owner.0)
        .fetch_optional(&self.pool)
        .await?
        .map(|row| {
            let policies = PoliciesRow {
                allowed_actions: row.allowed_actions,
                github_owned_allowed: row.github_owned_allowed,
                patterns_allowed: row.patterns_allowed,
                token_permission: row.token_permission,
                can_approve_reviews: row.can_approve_reviews,
                approval_policy: row.approval_policy,
            };
            (row.enabled_repositories, policies)
        });
        let Some((enabled, policies)) = row else { return Ok(OwnerActionsSettings::default()) };
        Ok(OwnerActionsSettings {
            enabled_repositories: EnabledRepositories::parse(&enabled).ok_or_else(|| corrupt("enabled repositories", &enabled))?,
            policies: ActionsPolicies::try_from(policies)?,
        })
    }

    pub async fn set_owner_settings(&self, owner: AccountId, settings: &OwnerActionsSettings, now: Timestamp) -> Result<(), StorageError> {
        let policies = &settings.policies;
        sqlx::query(
            "INSERT INTO actions_owner_settings (owner_id, enabled_repositories, allowed_actions, github_owned_allowed, \
             patterns_allowed, token_permission, can_approve_reviews, approval_policy, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
             ON CONFLICT (owner_id) DO UPDATE SET enabled_repositories = ?2, allowed_actions = ?3, github_owned_allowed = ?4, \
             patterns_allowed = ?5, token_permission = ?6, can_approve_reviews = ?7, approval_policy = ?8, updated_at = ?9",
        )
        .bind(owner.0)
        .bind(settings.enabled_repositories.as_str())
        .bind(policies.allowed_actions.as_str())
        .bind(policies.selected_actions.github_owned_allowed)
        .bind(patterns_json(&policies.selected_actions.patterns_allowed))
        .bind(policies.token_permission.as_str())
        .bind(policies.can_approve_reviews)
        .bind(policies.approval_policy.as_str())
        .bind(now.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The repositories the owner selected to run workflows, by id; only owned ones count.
    pub async fn owner_enabled_repositories(&self, owner: AccountId) -> Result<Vec<RepoId>, StorageError> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT e.repo_id FROM actions_owner_enabled_repositories e JOIN repositories r ON r.id = e.repo_id \
             WHERE e.owner_id = ?1 AND r.owner_id = ?1 ORDER BY e.repo_id",
        )
        .bind(owner.0)
        .fetch_all(&self.pool)
        .await?;
        Ok(ids.into_iter().map(RepoId).collect())
    }

    /// Replaces the owner's selected repositories with `repos` (all the owner's own).
    pub async fn set_owner_enabled_repositories(&self, owner: AccountId, repos: &[RepoId]) -> Result<(), StorageError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM actions_owner_enabled_repositories WHERE owner_id = ?").bind(owner.0).execute(&mut *tx).await?;
        for repo in repos {
            sqlx::query("INSERT INTO actions_owner_enabled_repositories (owner_id, repo_id) VALUES (?, ?) ON CONFLICT DO NOTHING")
                .bind(owner.0)
                .bind(repo.0)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Adds or removes one of the owner's selected repositories. Returns whether it changed.
    pub async fn select_owner_repository(&self, owner: AccountId, repo: RepoId, selected: bool) -> Result<bool, StorageError> {
        let result = if selected {
            sqlx::query("INSERT INTO actions_owner_enabled_repositories (owner_id, repo_id) VALUES (?, ?) ON CONFLICT DO NOTHING")
                .bind(owner.0)
                .bind(repo.0)
                .execute(&self.pool)
                .await?
        } else {
            sqlx::query("DELETE FROM actions_owner_enabled_repositories WHERE owner_id = ? AND repo_id = ?")
                .bind(owner.0)
                .bind(repo.0)
                .execute(&self.pool)
                .await?
        };
        Ok(result.rows_affected() == 1)
    }

    /// Turns Actions on or off for the repository.
    pub async fn set_enabled(&self, repo: RepoId, enabled: bool, now: Timestamp) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO actions_repositories (repo_id, enabled, updated_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT (repo_id) DO UPDATE SET enabled = ?2, updated_at = ?3",
        )
        .bind(repo.0)
        .bind(enabled)
        .bind(now.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Sets the default permission of the repository's task tokens.
    pub async fn set_token_permission(&self, repo: RepoId, permission: DefaultTokenPermission, now: Timestamp) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO actions_repositories (repo_id, token_permission, updated_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT (repo_id) DO UPDATE SET token_permission = ?2, updated_at = ?3",
        )
        .bind(repo.0)
        .bind(permission.as_str())
        .bind(now.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Whether an administrator allowed instance runners to take the owner's jobs.
    pub async fn instance_runners_allowed(&self, owner: AccountId) -> Result<bool, StorageError> {
        let allowed: Option<bool> = sqlx::query_scalar("SELECT instance_runners FROM actions_owner_policies WHERE owner_id = ?")
            .bind(owner.0)
            .fetch_optional(&self.pool)
            .await?;
        Ok(allowed.unwrap_or(false))
    }

    pub async fn set_instance_runners_allowed(
        &self,
        owner: AccountId,
        allowed: bool,
        by: AccountId,
        now: Timestamp,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO actions_owner_policies (owner_id, instance_runners, updated_by, updated_at) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (owner_id) DO UPDATE SET instance_runners = ?2, updated_by = ?3, updated_at = ?4",
        )
        .bind(owner.0)
        .bind(allowed)
        .bind(by.0)
        .bind(now.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The owners instance runners serve, by id.
    pub async fn owners_with_instance_runners(&self) -> Result<Vec<AccountId>, StorageError> {
        let owners: Vec<i64> =
            sqlx::query_scalar("SELECT owner_id FROM actions_owner_policies WHERE instance_runners = 1 ORDER BY owner_id")
                .fetch_all(&self.pool)
                .await?;
        Ok(owners.into_iter().map(AccountId).collect())
    }

    /// Trusts `user`'s pull requests in `repo`. Returns false when already trusted.
    pub async fn trust(&self, repo: RepoId, user: AccountId, by: AccountId, now: Timestamp) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "INSERT INTO actions_trusted_users (repo_id, user_id, approved_by, created_at) VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING",
        )
        .bind(repo.0)
        .bind(user.0)
        .bind(by.0)
        .bind(now.0)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn untrust(&self, repo: RepoId, user: AccountId) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM actions_trusted_users WHERE repo_id = ? AND user_id = ?")
            .bind(repo.0)
            .bind(user.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn is_trusted(&self, repo: RepoId, user: AccountId) -> Result<bool, StorageError> {
        let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM actions_trusted_users WHERE repo_id = ? AND user_id = ?")
            .bind(repo.0)
            .bind(user.0)
            .fetch_optional(&self.pool)
            .await?;
        Ok(found.is_some())
    }

    /// The repository's trusted users, oldest first.
    pub async fn trusted_users(&self, repo: RepoId) -> Result<Vec<TrustedUser>, StorageError> {
        let rows: Vec<(i64, i64, Option<i64>, i64)> = sqlx::query_as(
            "SELECT repo_id, user_id, approved_by, created_at FROM actions_trusted_users WHERE repo_id = ? ORDER BY created_at, user_id",
        )
        .bind(repo.0)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(repo, user, by, at)| TrustedUser {
                repo_id: RepoId(repo),
                user_id: AccountId(user),
                approved_by: by.map(AccountId),
                created_at: Timestamp(at),
            })
            .collect())
    }
}
