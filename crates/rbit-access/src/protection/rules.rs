//! Rule settings, their validation, and the rule shapes the decisions and stores share.

use super::BranchPattern;

/// Most rules per repository.
pub const MAX_RULES: usize = 50;
/// Most allowed users, and most allowed teams, per rule.
pub const MAX_ALLOWED: usize = 50;
/// Most required status checks per rule.
pub const MAX_REQUIRED_CONTEXTS: usize = 50;
/// Most approvals a rule may require.
pub const MAX_REQUIRED_APPROVALS: u8 = 10;

/// Stable primary key of a branch protection rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProtectionId(pub i64);

/// The flags of one rule. [`Default`] gives the defaults of a new rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "independent rule flags")]
pub struct RuleSettings {
    pub deny_force_push: bool,
    pub deny_deletion: bool,
    /// Updates of an existing branch must be merged pull requests (creating it is allowed).
    pub require_pull_request: bool,
    /// 0–[`MAX_REQUIRED_APPROVALS`]; above 0 requires `require_pull_request`.
    pub required_approvals: u8,
    pub dismiss_stale_approvals: bool,
    pub block_on_changes_requested: bool,
    /// Only the allowed users and members of the allowed teams may push.
    pub restrict_pushers: bool,
    /// Repository administrators pass this rule (default off).
    pub allow_admin_bypass: bool,
    /// Merging needs every required context to pass on the head commit (milestone 3, S).
    pub require_status_checks: bool,
}

impl Default for RuleSettings {
    fn default() -> Self {
        Self {
            deny_force_push: true,
            deny_deletion: true,
            require_pull_request: false,
            required_approvals: 0,
            dismiss_stale_approvals: false,
            block_on_changes_requested: true,
            restrict_pushers: false,
            allow_admin_bypass: false,
            require_status_checks: false,
        }
    }
}

/// Why rule settings were rejected. The message is shown to the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuleError {
    #[error("required approvals must be between 0 and {MAX_REQUIRED_APPROVALS}")]
    TooManyApprovals,
    #[error("required approvals need \"require a pull request\"")]
    ApprovalsWithoutPullRequest,
}

impl RuleSettings {
    pub fn validate(&self) -> Result<(), RuleError> {
        if self.required_approvals > MAX_REQUIRED_APPROVALS {
            return Err(RuleError::TooManyApprovals);
        }
        if self.required_approvals > 0 && !self.require_pull_request {
            return Err(RuleError::ApprovalsWithoutPullRequest);
        }
        Ok(())
    }
}

/// What the decisions need of a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectionRule<U, T, C> {
    pub pattern: BranchPattern,
    pub settings: RuleSettings,
    /// With `restrict_pushers`: users who may push.
    pub allowed_users: Vec<U>,
    /// With `restrict_pushers`: teams whose members may push.
    pub allowed_teams: Vec<T>,
    /// With `require_status_checks`: the contexts that must pass.
    pub required_contexts: Vec<C>,
}

impl<U, T, C> ProtectionRule<U, T, C> {
    pub fn matches(&self, branch: &str) -> bool {
        self.pattern.matches(branch)
    }
}

/// A rule as written: everything but its id and timestamps. Values are validated.
#[derive(Debug, Clone)]
pub struct RuleInput<U, T, C> {
    pub pattern: BranchPattern,
    pub settings: RuleSettings,
    pub users: Vec<U>,
    pub teams: Vec<T>,
    /// Required status check contexts (milestone 3, S).
    pub contexts: Vec<C>,
}

/// What a rule store did with a create or update.
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

/// The rule's protections in a few words, for the audit log.
pub fn summary(settings: &RuleSettings) -> String {
    let mut parts = Vec::new();
    if settings.deny_force_push {
        parts.push("no force pushes".to_owned());
    }
    if settings.deny_deletion {
        parts.push("no deletion".to_owned());
    }
    if settings.require_pull_request {
        parts.push(match settings.required_approvals {
            0 => "pull request".to_owned(),
            1 => "1 approval".to_owned(),
            n => format!("{n} approvals"),
        });
    }
    if settings.require_status_checks {
        parts.push("status checks".to_owned());
    }
    if settings.restrict_pushers {
        parts.push("restricted".to_owned());
    }
    if settings.allow_admin_bypass {
        parts.push("admins bypass".to_owned());
    }
    if parts.is_empty() { "no restrictions".to_owned() } else { parts.join(", ") }
}
