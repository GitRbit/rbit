//! Append-only security audit log.
//!
//! Milestone 1 workstream S owns this file. Keep row types here rather than in `rows.rs`.
//!
//! Entries never change once written and never reference other rows: actor and target
//! names are copied in, so the log stays readable after accounts are renamed or deleted.

use rustit_core::{AccountId, Timestamp};
use sqlx::SqlitePool;

use super::admin::{ListFilter, Listing};
use crate::StorageError;

/// A group of related actions, used to filter the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditCategory {
    Authentication,
    TwoFactor,
    Sessions,
    Tokens,
    /// Collaborators, teams, transfers, webhooks and branch protection (milestone 2).
    RepositoryAccess,
    /// Issues and pull requests deleted (milestone 2 wave B).
    RepositoryContent,
    Administration,
}

impl AuditCategory {
    pub const ALL: [Self; 7] = [
        Self::Authentication,
        Self::TwoFactor,
        Self::Sessions,
        Self::Tokens,
        Self::RepositoryAccess,
        Self::RepositoryContent,
        Self::Administration,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Authentication => "authentication",
            Self::TwoFactor => "two_factor",
            Self::Sessions => "sessions",
            Self::Tokens => "tokens",
            Self::RepositoryAccess => "repository_access",
            Self::RepositoryContent => "repository_content",
            Self::Administration => "administration",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|category| category.as_str() == value)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Authentication => "Sign-in",
            Self::TwoFactor => "Two-factor",
            Self::Sessions => "Sessions",
            Self::Tokens => "Access tokens",
            Self::RepositoryAccess => "Repository access",
            Self::RepositoryContent => "Repository content",
            Self::Administration => "Administration",
        }
    }
}

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditAction {
    SignIn,
    SignInFailed,
    TwoFactorFailed,
    RecoveryCodeUsed,
    TwoFactorEnabled,
    TwoFactorDisabled,
    RecoveryCodesRegenerated,
    SessionRevoked,
    OtherSessionsRevoked,
    SessionsRevokedAfterPasswordChange,
    PasswordChanged,
    PasswordReset,
    TokenCreated,
    TokenRevoked,
    UserCreated,
    AdminGranted,
    AdminRevoked,
    UserSuspended,
    UserUnsuspended,
    UserDeleted,
    PasswordResetSent,
    PasswordSet,
    RepositoryDeleted,
    InstanceSettingsChanged,
    CollaboratorAdded,
    CollaboratorChanged,
    CollaboratorRemoved,
    TeamCreated,
    TeamChanged,
    TeamDeleted,
    RepositoryTransferred,
    WebhookCreated,
    WebhookChanged,
    WebhookDeleted,
    OrgPermissionChanged,
    BranchProtectionCreated,
    BranchProtectionChanged,
    BranchProtectionDeleted,
    BranchProtectionBypassed,
    IssueDeleted,
}

impl AuditAction {
    pub const ALL: [Self; 40] = [
        Self::SignIn,
        Self::SignInFailed,
        Self::TwoFactorFailed,
        Self::RecoveryCodeUsed,
        Self::TwoFactorEnabled,
        Self::TwoFactorDisabled,
        Self::RecoveryCodesRegenerated,
        Self::SessionRevoked,
        Self::OtherSessionsRevoked,
        Self::SessionsRevokedAfterPasswordChange,
        Self::PasswordChanged,
        Self::PasswordReset,
        Self::TokenCreated,
        Self::TokenRevoked,
        Self::UserCreated,
        Self::AdminGranted,
        Self::AdminRevoked,
        Self::UserSuspended,
        Self::UserUnsuspended,
        Self::UserDeleted,
        Self::PasswordResetSent,
        Self::PasswordSet,
        Self::RepositoryDeleted,
        Self::InstanceSettingsChanged,
        Self::CollaboratorAdded,
        Self::CollaboratorChanged,
        Self::CollaboratorRemoved,
        Self::TeamCreated,
        Self::TeamChanged,
        Self::TeamDeleted,
        Self::RepositoryTransferred,
        Self::WebhookCreated,
        Self::WebhookChanged,
        Self::WebhookDeleted,
        Self::OrgPermissionChanged,
        Self::BranchProtectionCreated,
        Self::BranchProtectionChanged,
        Self::BranchProtectionDeleted,
        Self::BranchProtectionBypassed,
        Self::IssueDeleted,
    ];

    /// Stable identifier stored in the database.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SignIn => "sign_in",
            Self::SignInFailed => "sign_in_failed",
            Self::TwoFactorFailed => "two_factor_failed",
            Self::RecoveryCodeUsed => "recovery_code_used",
            Self::TwoFactorEnabled => "two_factor_enabled",
            Self::TwoFactorDisabled => "two_factor_disabled",
            Self::RecoveryCodesRegenerated => "recovery_codes_regenerated",
            Self::SessionRevoked => "session_revoked",
            Self::OtherSessionsRevoked => "other_sessions_revoked",
            Self::SessionsRevokedAfterPasswordChange => "sessions_revoked_password_change",
            Self::PasswordChanged => "password_changed",
            Self::PasswordReset => "password_reset",
            Self::TokenCreated => "token_created",
            Self::TokenRevoked => "token_revoked",
            Self::UserCreated => "user_created",
            Self::AdminGranted => "admin_granted",
            Self::AdminRevoked => "admin_revoked",
            Self::UserSuspended => "user_suspended",
            Self::UserUnsuspended => "user_unsuspended",
            Self::UserDeleted => "user_deleted",
            Self::PasswordResetSent => "password_reset_sent",
            Self::PasswordSet => "password_set",
            Self::RepositoryDeleted => "repository_deleted",
            Self::InstanceSettingsChanged => "instance_settings_changed",
            Self::CollaboratorAdded => "collaborator_added",
            Self::CollaboratorChanged => "collaborator_changed",
            Self::CollaboratorRemoved => "collaborator_removed",
            Self::TeamCreated => "team_created",
            Self::TeamChanged => "team_changed",
            Self::TeamDeleted => "team_deleted",
            Self::RepositoryTransferred => "repository_transferred",
            Self::WebhookCreated => "webhook_created",
            Self::WebhookChanged => "webhook_changed",
            Self::WebhookDeleted => "webhook_deleted",
            Self::OrgPermissionChanged => "org_permission_changed",
            Self::BranchProtectionCreated => "branch_protection_created",
            Self::BranchProtectionChanged => "branch_protection_changed",
            Self::BranchProtectionDeleted => "branch_protection_deleted",
            Self::BranchProtectionBypassed => "branch_protection_bypassed",
            Self::IssueDeleted => "issue_deleted",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.as_str() == value)
    }

    pub const fn category(self) -> AuditCategory {
        match self {
            Self::SignIn
            | Self::SignInFailed
            | Self::TwoFactorFailed
            | Self::RecoveryCodeUsed
            | Self::PasswordChanged
            | Self::PasswordReset => AuditCategory::Authentication,
            Self::TwoFactorEnabled | Self::TwoFactorDisabled | Self::RecoveryCodesRegenerated => AuditCategory::TwoFactor,
            Self::SessionRevoked | Self::OtherSessionsRevoked | Self::SessionsRevokedAfterPasswordChange => AuditCategory::Sessions,
            Self::TokenCreated | Self::TokenRevoked => AuditCategory::Tokens,
            Self::UserCreated
            | Self::AdminGranted
            | Self::AdminRevoked
            | Self::UserSuspended
            | Self::UserUnsuspended
            | Self::UserDeleted
            | Self::PasswordResetSent
            | Self::PasswordSet
            | Self::RepositoryDeleted
            | Self::InstanceSettingsChanged => AuditCategory::Administration,
            Self::CollaboratorAdded
            | Self::CollaboratorChanged
            | Self::CollaboratorRemoved
            | Self::TeamCreated
            | Self::TeamChanged
            | Self::TeamDeleted
            | Self::RepositoryTransferred
            | Self::WebhookCreated
            | Self::WebhookChanged
            | Self::WebhookDeleted
            | Self::OrgPermissionChanged
            | Self::BranchProtectionCreated
            | Self::BranchProtectionChanged
            | Self::BranchProtectionDeleted
            | Self::BranchProtectionBypassed => AuditCategory::RepositoryAccess,
            Self::IssueDeleted => AuditCategory::RepositoryContent,
        }
    }

    /// Human-readable description, e.g. "Signed in".
    pub const fn label(self) -> &'static str {
        match self {
            Self::SignIn => "Signed in",
            Self::SignInFailed => "Failed sign-in",
            Self::TwoFactorFailed => "Wrong two-factor code",
            Self::RecoveryCodeUsed => "Signed in with a recovery code",
            Self::TwoFactorEnabled => "Enabled two-factor authentication",
            Self::TwoFactorDisabled => "Disabled two-factor authentication",
            Self::RecoveryCodesRegenerated => "Regenerated recovery codes",
            Self::SessionRevoked => "Revoked a session",
            Self::OtherSessionsRevoked => "Signed out other sessions",
            Self::SessionsRevokedAfterPasswordChange => "Sessions ended after a password change",
            Self::PasswordChanged => "Changed password",
            Self::PasswordReset => "Reset password by email link",
            Self::PasswordResetSent => "Sent a password reset email",
            Self::PasswordSet => "Set a user's password",
            Self::TokenCreated => "Created an access token",
            Self::TokenRevoked => "Revoked an access token",
            Self::UserCreated => "Created a user",
            Self::AdminGranted => "Granted administrator rights",
            Self::AdminRevoked => "Removed administrator rights",
            Self::UserSuspended => "Suspended a user",
            Self::UserUnsuspended => "Lifted a suspension",
            Self::UserDeleted => "Deleted a user",
            Self::RepositoryDeleted => "Deleted a repository",
            Self::InstanceSettingsChanged => "Changed instance settings",
            Self::CollaboratorAdded => "Added a collaborator",
            Self::CollaboratorChanged => "Changed a collaborator's permission",
            Self::CollaboratorRemoved => "Removed a collaborator",
            Self::TeamCreated => "Created a team",
            Self::TeamChanged => "Changed a team's members or repositories",
            Self::TeamDeleted => "Deleted a team",
            Self::RepositoryTransferred => "Transferred a repository",
            Self::WebhookCreated => "Created a webhook",
            Self::WebhookChanged => "Changed a webhook",
            Self::WebhookDeleted => "Deleted a webhook",
            Self::OrgPermissionChanged => "Changed an organization's member permission",
            Self::BranchProtectionCreated => "Added a branch protection rule",
            Self::BranchProtectionChanged => "Changed a branch protection rule",
            Self::BranchProtectionDeleted => "Deleted a branch protection rule",
            Self::BranchProtectionBypassed => "Bypassed branch protection",
            Self::IssueDeleted => "Deleted an issue",
        }
    }

    /// Failures are highlighted in the log.
    pub const fn is_failure(self) -> bool {
        matches!(self, Self::SignInFailed | Self::TwoFactorFailed)
    }
}

/// An entry to append.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAuditEvent {
    pub action: AuditAction,
    /// Who did it, when known (failed sign-ins only know the attempted login, as `actor_name`).
    pub actor_id: Option<AccountId>,
    pub actor_name: Option<String>,
    /// What it was done to (a username, repository, token name…).
    pub target_name: Option<String>,
    /// The client address, as text.
    pub ip_address: Option<String>,
    pub detail: String,
}

/// A stored entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub id: i64,
    pub created_at: Timestamp,
    pub action: AuditAction,
    pub actor_id: Option<AccountId>,
    pub actor_name: Option<String>,
    pub target_name: Option<String>,
    pub ip_address: Option<String>,
    pub detail: String,
}

#[derive(Debug, sqlx::FromRow)]
struct AuditEventRow {
    id: i64,
    created_at: i64,
    action: String,
    actor_id: Option<i64>,
    actor_name: Option<String>,
    target_name: Option<String>,
    ip_address: Option<String>,
    detail: String,
}

impl TryFrom<AuditEventRow> for AuditEvent {
    type Error = StorageError;

    fn try_from(row: AuditEventRow) -> Result<Self, Self::Error> {
        let action = AuditAction::parse(&row.action).ok_or_else(|| StorageError::Corrupt(format!("audit action {:?}", row.action)))?;
        Ok(Self {
            id: row.id,
            created_at: Timestamp(row.created_at),
            action,
            actor_id: row.actor_id.map(AccountId),
            actor_name: row.actor_name,
            target_name: row.target_name,
            ip_address: row.ip_address,
            detail: row.detail,
        })
    }
}

/// Matches `?1` = category ('' for all) and `?2` = search ('' for all, `?3` its LIKE pattern).
macro_rules! audit_filter {
    () => {
        "WHERE (?1 = '' OR category = ?1) \
         AND (?2 = '' OR actor_name LIKE ?3 ESCAPE '\\' OR target_name LIKE ?3 ESCAPE '\\' OR ip_address LIKE ?3 ESCAPE '\\')"
    };
}

#[derive(Debug, Clone)]
pub struct AuditStore {
    pool: SqlitePool,
}

impl AuditStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn append(&self, event: &NewAuditEvent) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO audit_events (created_at, category, action, actor_id, actor_name, target_name, ip_address, detail) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Timestamp::now().seconds())
        .bind(event.action.category().as_str())
        .bind(event.action.as_str())
        .bind(event.actor_id.map(|id| id.0))
        .bind(event.actor_name.as_deref())
        .bind(event.target_name.as_deref())
        .bind(event.ip_address.as_deref())
        .bind(&event.detail)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Newest first. `filter.search` matches actor, target or address (substring,
    /// case-insensitive for ASCII).
    pub async fn list(&self, category: Option<AuditCategory>, filter: ListFilter<'_>) -> Result<Listing<AuditEvent>, StorageError> {
        let category = category.map_or("", AuditCategory::as_str);
        let pattern = like_pattern(filter.search);
        let rows: Vec<AuditEventRow> = sqlx::query_as(concat!(
            "SELECT id, created_at, action, actor_id, actor_name, target_name, ip_address, detail FROM audit_events ",
            audit_filter!(),
            " ORDER BY id DESC LIMIT ?4 OFFSET ?5"
        ))
        .bind(category)
        .bind(filter.search)
        .bind(&pattern)
        .bind(filter.limit)
        .bind(filter.offset)
        .fetch_all(&self.pool)
        .await?;
        let total: i64 = sqlx::query_scalar(concat!("SELECT COUNT(*) FROM audit_events ", audit_filter!()))
            .bind(category)
            .bind(filter.search)
            .bind(&pattern)
            .fetch_one(&self.pool)
            .await?;
        Ok(Listing { items: rows.into_iter().map(AuditEvent::try_from).collect::<Result<_, _>>()?, total })
    }

    /// The account's own security events (sign-ins, 2FA, sessions, tokens), newest first.
    pub async fn list_for_actor(&self, actor: AccountId, limit: i64) -> Result<Vec<AuditEvent>, StorageError> {
        let rows: Vec<AuditEventRow> = sqlx::query_as(
            "SELECT id, created_at, action, actor_id, actor_name, target_name, ip_address, detail FROM audit_events \
             WHERE actor_id = ? ORDER BY id DESC LIMIT ?",
        )
        .bind(actor.0)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(AuditEvent::try_from).collect()
    }
}

/// `%search%` with LIKE wildcards in the input escaped.
fn like_pattern(search: &str) -> String {
    let mut pattern = String::with_capacity(search.len() + 2);
    pattern.push('%');
    for c in search.chars() {
        if matches!(c, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_round_trip_and_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for action in AuditAction::ALL {
            assert_eq!(AuditAction::parse(action.as_str()), Some(action));
            assert!(seen.insert(action.as_str()), "{} is duplicated", action.as_str());
        }
        for category in AuditCategory::ALL {
            assert_eq!(AuditCategory::parse(category.as_str()), Some(category));
            assert!(AuditAction::ALL.iter().any(|action| action.category() == category), "{category:?} has no actions");
        }
        assert_eq!(AuditAction::parse("nope"), None);
    }

    #[test]
    fn escapes_like_wildcards() {
        assert_eq!(like_pattern("a_b%c\\"), "%a\\_b\\%c\\\\%");
    }
}
