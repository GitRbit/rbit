//! Organisations and their members.
//!
//! An organisation is a row in `accounts` (kind `organization`) plus an `org_profiles`
//! row and its `org_members`. Row types live here rather than in `rows.rs`.

use rustit_core::{AccountId, AccountKind, AccountName, OrgMember, OrgMembership, OrgRole, Organization, Timestamp};
use sqlx::{SqliteExecutor, SqlitePool};

use super::rows::AccountRow;
use crate::StorageError;

/// Builds a static `SELECT` of organisations (account ⋈ profile) with the given trailing clause.
macro_rules! select_org {
    ($clause:literal) => {
        concat!(
            "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at, p.description, p.website \
             FROM accounts a LEFT JOIN org_profiles p ON p.account_id = a.id \
             WHERE a.kind = 'organization' ",
            $clause
        )
    };
}

/// Everything needed to create an organisation. Values are already validated.
#[derive(Debug)]
pub struct NewOrganization {
    pub name: AccountName,
    pub display_name: Option<String>,
    pub description: Option<String>,
    /// Becomes the first owner.
    pub creator: AccountId,
}

/// New profile values. Values are already validated; `None` clears a field.
#[derive(Debug, Default)]
pub struct OrgProfileUpdate {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub website: Option<String>,
}

/// Result of a membership change that must keep at least one owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembershipChange {
    Done,
    /// The user is not a member of the organisation.
    NotMember,
    /// Refused: the change would leave the organisation without an owner.
    LastOwner,
}

#[derive(Debug, sqlx::FromRow)]
struct OrgRow {
    #[sqlx(flatten)]
    account: AccountRow,
    description: Option<String>,
    website: Option<String>,
}

impl TryFrom<OrgRow> for Organization {
    type Error = StorageError;

    fn try_from(row: OrgRow) -> Result<Self, Self::Error> {
        Ok(Self { account: row.account.try_into()?, description: row.description, website: row.website })
    }
}

/// An account joined with a membership role (the member's account or the organisation's).
#[derive(Debug, sqlx::FromRow)]
struct RoleRow {
    #[sqlx(flatten)]
    account: AccountRow,
    role: String,
}

impl RoleRow {
    fn into_parts(self) -> Result<(rustit_core::Account, OrgRole), StorageError> {
        Ok((self.account.try_into()?, parse_role(&self.role)?))
    }
}

fn parse_role(value: &str) -> Result<OrgRole, StorageError> {
    OrgRole::parse(value).ok_or_else(|| StorageError::Corrupt(format!("organization role: {value}")))
}

/// `user`'s role in `org`, if they are a member. Shared with the repository store, which
/// needs it for membership-aware access checks.
pub(crate) async fn role_of<'e>(
    executor: impl SqliteExecutor<'e>,
    org: AccountId,
    user: AccountId,
) -> Result<Option<OrgRole>, StorageError> {
    let role: Option<String> = sqlx::query_scalar("SELECT role FROM org_members WHERE org_id = ? AND user_id = ?")
        .bind(org.0)
        .bind(user.0)
        .fetch_optional(executor)
        .await?;
    role.as_deref().map(parse_role).transpose()
}

#[derive(Debug, Clone)]
pub struct OrgStore {
    pool: SqlitePool,
}

impl OrgStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Creates the account, its profile and the creator's owner membership atomically.
    pub async fn create(&self, new: NewOrganization) -> Result<Organization, StorageError> {
        let now = Timestamp::now().seconds();
        let mut tx = self.pool.begin().await?;

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO accounts (kind, name, display_name, created_at, updated_at) VALUES (?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(AccountKind::Organization.as_str())
        .bind(new.name.as_str())
        .bind(new.display_name.as_deref())
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| StorageError::from_insert(e, "name"))?;

        sqlx::query("INSERT INTO org_profiles (account_id, description) VALUES (?, ?)")
            .bind(id)
            .bind(new.description.as_deref())
            .execute(&mut *tx)
            .await?;

        sqlx::query("INSERT INTO org_members (org_id, user_id, role) VALUES (?, ?, ?)")
            .bind(id)
            .bind(new.creator.0)
            .bind(OrgRole::Owner.as_str())
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(Organization {
            account: rustit_core::Account {
                id: AccountId(id),
                kind: AccountKind::Organization,
                name: new.name,
                display_name: new.display_name,
                bio: None,
                created_at: Timestamp(now),
            },
            description: new.description,
            website: None,
        })
    }

    /// The organisation called `name` (case-insensitively). Users are never returned.
    pub async fn find_by_name(&self, name: &str) -> Result<Option<Organization>, StorageError> {
        let row: Option<OrgRow> = sqlx::query_as(select_org!("AND a.name = ?")).bind(name).fetch_optional(&self.pool).await?;
        row.map(Organization::try_from).transpose()
    }

    pub async fn find(&self, id: AccountId) -> Result<Option<Organization>, StorageError> {
        let row: Option<OrgRow> = sqlx::query_as(select_org!("AND a.id = ?")).bind(id.0).fetch_optional(&self.pool).await?;
        row.map(Organization::try_from).transpose()
    }

    pub async fn update_profile(&self, org: AccountId, update: OrgProfileUpdate) -> Result<(), StorageError> {
        let now = Timestamp::now().seconds();
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE accounts SET display_name = ?, updated_at = ? WHERE id = ? AND kind = 'organization'")
            .bind(update.display_name.as_deref())
            .bind(now)
            .bind(org.0)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO org_profiles (account_id, description, website) VALUES (?1, ?2, ?3) \
             ON CONFLICT (account_id) DO UPDATE SET description = ?2, website = ?3",
        )
        .bind(org.0)
        .bind(update.description.as_deref())
        .bind(update.website.as_deref())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Deletes the organisation, its profile and memberships, but only while it owns no
    /// repositories (their bare repositories on disk would otherwise be orphaned).
    /// Returns whether it was deleted.
    pub async fn delete_if_empty(&self, org: AccountId) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "DELETE FROM accounts WHERE id = ?1 AND kind = 'organization' \
             AND NOT EXISTS (SELECT 1 FROM repositories WHERE owner_id = ?1)",
        )
        .bind(org.0)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn count_repositories(&self, org: AccountId) -> Result<i64, StorageError> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM repositories WHERE owner_id = ?").bind(org.0).fetch_one(&self.pool).await?)
    }

    /// Members of `org`: owners first, then by name.
    pub async fn members(&self, org: AccountId) -> Result<Vec<OrgMember>, StorageError> {
        let rows: Vec<RoleRow> = sqlx::query_as(
            "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at, m.role \
             FROM org_members m JOIN accounts a ON a.id = m.user_id \
             WHERE m.org_id = ? ORDER BY m.role = 'owner' DESC, a.name COLLATE NOCASE",
        )
        .bind(org.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(|row| row.into_parts().map(|(user, role)| OrgMember { user, role })).collect()
    }

    /// Organisations `user` belongs to, by name.
    pub async fn memberships_of(&self, user: AccountId) -> Result<Vec<OrgMembership>, StorageError> {
        let rows: Vec<RoleRow> = sqlx::query_as(
            "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at, m.role \
             FROM org_members m JOIN accounts a ON a.id = m.org_id \
             WHERE m.user_id = ? ORDER BY a.name COLLATE NOCASE",
        )
        .bind(user.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(|row| row.into_parts().map(|(org, role)| OrgMembership { org, role })).collect()
    }

    pub async fn role_of(&self, org: AccountId, user: AccountId) -> Result<Option<OrgRole>, StorageError> {
        role_of(&self.pool, org, user).await
    }

    /// Adds `user` with `role`. [`StorageError::Conflict`] if they already are a member.
    pub async fn add_member(&self, org: AccountId, user: AccountId, role: OrgRole) -> Result<(), StorageError> {
        sqlx::query("INSERT INTO org_members (org_id, user_id, role) VALUES (?, ?, ?)")
            .bind(org.0)
            .bind(user.0)
            .bind(role.as_str())
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::from_insert(e, "member"))?;
        Ok(())
    }

    /// Changes `user`'s role. Demoting the last owner is refused in the same statement,
    /// so concurrent demotions can never leave the organisation ownerless.
    pub async fn set_role(&self, org: AccountId, user: AccountId, role: OrgRole) -> Result<MembershipChange, StorageError> {
        let result = sqlx::query(
            "UPDATE org_members SET role = ?3 WHERE org_id = ?1 AND user_id = ?2 \
             AND (?3 = 'owner' OR role <> 'owner' \
                  OR (SELECT COUNT(*) FROM org_members WHERE org_id = ?1 AND role = 'owner') > 1)",
        )
        .bind(org.0)
        .bind(user.0)
        .bind(role.as_str())
        .execute(&self.pool)
        .await?;
        self.outcome(org, user, result.rows_affected()).await
    }

    /// Removes `user` from `org`. Removing the last owner is refused in the same statement.
    pub async fn remove_member(&self, org: AccountId, user: AccountId) -> Result<MembershipChange, StorageError> {
        let result = sqlx::query(
            "DELETE FROM org_members WHERE org_id = ?1 AND user_id = ?2 \
             AND (role <> 'owner' OR (SELECT COUNT(*) FROM org_members WHERE org_id = ?1 AND role = 'owner') > 1)",
        )
        .bind(org.0)
        .bind(user.0)
        .execute(&self.pool)
        .await?;
        self.outcome(org, user, result.rows_affected()).await
    }

    /// Explains why a guarded statement touched no row.
    async fn outcome(&self, org: AccountId, user: AccountId, rows_affected: u64) -> Result<MembershipChange, StorageError> {
        if rows_affected > 0 {
            return Ok(MembershipChange::Done);
        }
        Ok(match self.role_of(org, user).await? {
            None => MembershipChange::NotMember,
            Some(_) => MembershipChange::LastOwner,
        })
    }
}
