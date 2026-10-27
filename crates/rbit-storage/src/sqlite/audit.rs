//! Append-only security audit log.
//!
//! Milestone 1 workstream S owns this file. Keep row types here rather than in `rows.rs`.
//!
//! Entries never change once written and never reference other rows: actor and target
//! names are copied in, so the log stays readable after accounts are renamed or deleted.

use super::pool::Pool;
use rbit_core::{AccountId, Timestamp};

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
    /// Issues and pull requests deleted (milestone 2 wave B), repositories archived and
    /// unarchived.
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
    UsernameChanged,
    SignIn,
    SignInFailed,
    TwoFactorFailed,
    RecoveryCodeUsed,
    TwoFactorEnabled,
    TwoFactorDisabled,
    RecoveryCodesRegenerated,
    PasskeyAdded,
    PasskeyRemoved,
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
    // Milestone 3: external sign-in (O).
    ExternalIdentityLinked,
    ExternalIdentityUnlinked,
    // Actions: triggers (T).
    ActionsRunApproved,
    ActionsUserTrusted,
    // Actions: environments.
    ActionsEnvironmentChanged,
    ActionsEnvironmentDeleted,
    ActionsDeploymentApproved,
    ActionsDeploymentRejected,
    // Actions: interface and secrets (U).
    ActionsSecretSet,
    ActionsSecretDeleted,
    ActionsVariableSet,
    ActionsVariableDeleted,
    ActionsEnabled,
    ActionsDisabled,
    ActionsTokenPermissionChanged,
    ActionsUserUntrusted,
    // Actions: S.
    ActionsRegistrationTokenCreated,
    ActionsRegistrationTokenRevoked,
    ActionsRunnerRegistered,
    ActionsRunnerDisabled,
    ActionsRunnerEnabled,
    ActionsRunnerRemoved,
    RepositoryArchived,
    RepositoryUnarchived,
}

impl AuditAction {
    pub const ALL: [Self; 67] = [
        Self::UsernameChanged,
        Self::SignIn,
        Self::SignInFailed,
        Self::TwoFactorFailed,
        Self::RecoveryCodeUsed,
        Self::TwoFactorEnabled,
        Self::TwoFactorDisabled,
        Self::RecoveryCodesRegenerated,
        Self::PasskeyAdded,
        Self::PasskeyRemoved,
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
        Self::ExternalIdentityLinked,
        Self::ExternalIdentityUnlinked,
        // Actions: triggers (T).
        Self::ActionsRunApproved,
        Self::ActionsUserTrusted,
        Self::ActionsEnvironmentChanged,
        Self::ActionsEnvironmentDeleted,
        Self::ActionsDeploymentApproved,
        Self::ActionsDeploymentRejected,
        // Actions: interface and secrets (U).
        Self::ActionsSecretSet,
        Self::ActionsSecretDeleted,
        Self::ActionsVariableSet,
        Self::ActionsVariableDeleted,
        Self::ActionsEnabled,
        Self::ActionsDisabled,
        Self::ActionsTokenPermissionChanged,
        Self::ActionsUserUntrusted,
        // Actions: S.
        Self::ActionsRegistrationTokenCreated,
        Self::ActionsRegistrationTokenRevoked,
        Self::ActionsRunnerRegistered,
        Self::ActionsRunnerDisabled,
        Self::ActionsRunnerEnabled,
        Self::ActionsRunnerRemoved,
        Self::RepositoryArchived,
        Self::RepositoryUnarchived,
    ];

    /// Stable identifier stored in the database.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UsernameChanged => "username_changed",
            Self::SignIn => "sign_in",
            Self::SignInFailed => "sign_in_failed",
            Self::TwoFactorFailed => "two_factor_failed",
            Self::RecoveryCodeUsed => "recovery_code_used",
            Self::TwoFactorEnabled => "two_factor_enabled",
            Self::TwoFactorDisabled => "two_factor_disabled",
            Self::RecoveryCodesRegenerated => "recovery_codes_regenerated",
            Self::PasskeyAdded => "passkey_added",
            Self::PasskeyRemoved => "passkey_removed",
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
            Self::ExternalIdentityLinked => "external_identity_linked",
            Self::ExternalIdentityUnlinked => "external_identity_unlinked",
            // Actions: triggers (T).
            Self::ActionsRunApproved => "actions_run_approved",
            Self::ActionsUserTrusted => "actions_user_trusted",
            Self::ActionsEnvironmentChanged => "actions_environment_changed",
            Self::ActionsEnvironmentDeleted => "actions_environment_deleted",
            Self::ActionsDeploymentApproved => "actions_deployment_approved",
            Self::ActionsDeploymentRejected => "actions_deployment_rejected",
            // Actions: interface and secrets (U).
            Self::ActionsSecretSet => "actions_secret_set",
            Self::ActionsSecretDeleted => "actions_secret_deleted",
            Self::ActionsVariableSet => "actions_variable_set",
            Self::ActionsVariableDeleted => "actions_variable_deleted",
            Self::ActionsEnabled => "actions_enabled",
            Self::ActionsDisabled => "actions_disabled",
            Self::ActionsTokenPermissionChanged => "actions_token_permission_changed",
            Self::ActionsUserUntrusted => "actions_user_untrusted",
            // Actions: S.
            Self::ActionsRegistrationTokenCreated => "actions_registration_token_created",
            Self::ActionsRegistrationTokenRevoked => "actions_registration_token_revoked",
            Self::ActionsRunnerRegistered => "actions_runner_registered",
            Self::ActionsRunnerDisabled => "actions_runner_disabled",
            Self::ActionsRunnerEnabled => "actions_runner_enabled",
            Self::ActionsRunnerRemoved => "actions_runner_removed",
            Self::RepositoryArchived => "repository_archived",
            Self::RepositoryUnarchived => "repository_unarchived",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.as_str() == value)
    }

    pub const fn category(self) -> AuditCategory {
        match self {
            Self::SignIn
            | Self::UsernameChanged
            | Self::SignInFailed
            | Self::TwoFactorFailed
            | Self::RecoveryCodeUsed
            | Self::PasswordChanged
            | Self::PasswordReset
            | Self::ExternalIdentityLinked
            | Self::ExternalIdentityUnlinked => AuditCategory::Authentication,
            Self::TwoFactorEnabled
            | Self::TwoFactorDisabled
            | Self::RecoveryCodesRegenerated
            | Self::PasskeyAdded
            | Self::PasskeyRemoved => AuditCategory::TwoFactor,
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
            | Self::BranchProtectionBypassed
            // Actions: triggers (T).
            | Self::ActionsRunApproved
            | Self::ActionsUserTrusted
            | Self::ActionsEnvironmentChanged
            | Self::ActionsEnvironmentDeleted
            | Self::ActionsDeploymentApproved
            | Self::ActionsDeploymentRejected
            // Actions: interface and secrets (U).
            | Self::ActionsSecretSet
            | Self::ActionsSecretDeleted
            | Self::ActionsVariableSet
            | Self::ActionsVariableDeleted
            | Self::ActionsEnabled
            | Self::ActionsDisabled
            | Self::ActionsTokenPermissionChanged
            | Self::ActionsUserUntrusted
            // Actions: S.
            | Self::ActionsRegistrationTokenCreated
            | Self::ActionsRegistrationTokenRevoked
            | Self::ActionsRunnerRegistered
            | Self::ActionsRunnerDisabled
            | Self::ActionsRunnerEnabled
            | Self::ActionsRunnerRemoved => AuditCategory::RepositoryAccess,
            Self::IssueDeleted | Self::RepositoryArchived | Self::RepositoryUnarchived => AuditCategory::RepositoryContent,
        }
    }

    /// Human-readable description, e.g. "Signed in".
    pub const fn label(self) -> &'static str {
        match self {
            Self::UsernameChanged => "Changed username",
            Self::SignIn => "Signed in",
            Self::SignInFailed => "Failed sign-in",
            Self::TwoFactorFailed => "Wrong two-factor code",
            Self::RecoveryCodeUsed => "Signed in with a recovery code",
            Self::TwoFactorEnabled => "Enabled two-factor authentication",
            Self::TwoFactorDisabled => "Disabled two-factor authentication",
            Self::RecoveryCodesRegenerated => "Regenerated recovery codes",
            Self::PasskeyAdded => "Added a passkey",
            Self::PasskeyRemoved => "Removed a passkey",
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
            Self::ExternalIdentityLinked => "Linked a sign-in provider",
            Self::ExternalIdentityUnlinked => "Unlinked a sign-in provider",
            // Actions: triggers (T).
            Self::ActionsRunApproved => "Approved a workflow run",
            Self::ActionsUserTrusted => "Trusted a pull request author's workflow runs",
            Self::ActionsEnvironmentChanged => "Changed a deployment environment",
            Self::ActionsEnvironmentDeleted => "Deleted a deployment environment",
            Self::ActionsDeploymentApproved => "Approved a deployment",
            Self::ActionsDeploymentRejected => "Rejected a deployment",
            // Actions: interface and secrets (U).
            Self::ActionsSecretSet => "Set an Actions secret",
            Self::ActionsSecretDeleted => "Deleted an Actions secret",
            Self::ActionsVariableSet => "Set an Actions variable",
            Self::ActionsVariableDeleted => "Deleted an Actions variable",
            Self::ActionsEnabled => "Turned Actions on for a repository",
            Self::ActionsDisabled => "Turned Actions off for a repository",
            Self::ActionsTokenPermissionChanged => "Changed the default workflow token permission",
            Self::ActionsUserUntrusted => "Stopped trusting a pull request author's workflow runs",
            // Actions: S.
            Self::ActionsRegistrationTokenCreated => "Created a runner registration token",
            Self::ActionsRegistrationTokenRevoked => "Revoked a runner registration token",
            Self::ActionsRunnerRegistered => "Registered a runner",
            Self::ActionsRunnerDisabled => "Disabled a runner",
            Self::ActionsRunnerEnabled => "Enabled a runner",
            Self::ActionsRunnerRemoved => "Removed a runner",
            Self::RepositoryArchived => "Archived a repository",
            Self::RepositoryUnarchived => "Unarchived a repository",
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
    pool: Pool,
}

impl AuditStore {
    pub(crate) fn new(pool: Pool) -> Self {
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
