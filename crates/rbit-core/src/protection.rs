//! Branch protection compatibility path: patterns, rules and the push and merge decisions
//! live in [`rbit_access::protection`]. This module fixes their identifiers to the domain
//! ones and keeps the stored rule record and the push-gate actor.

use rbit_access::protection as rules;

use crate::{
    Account, AccountId, RefUpdate, RepoAccess, RepoId, Timestamp, User,
    membership::{Team, TeamId},
    status::{StatusContext, StatusError},
};

pub use rules::{
    BranchPattern, Bypass, MAX_ALLOWED, MAX_PATTERN_LEN, MAX_REQUIRED_APPROVALS, MAX_RULES, PULL_REFS, PatternError, ProtectionId,
    PushDecision, PushVerdict, Refusal, RuleChange, RuleError, RuleSettings, decide_push, is_protected, may_delete, merge_rules, reasons,
};

/// What the decisions need of a rule, over domain identifiers.
pub type ProtectionRule = rules::ProtectionRule<AccountId, TeamId, StatusContext>;
/// What merging a pull request into a branch requires of the merger.
pub type MergeRules = rules::MergeRules<StatusContext>;
/// The pusher as the decisions see them.
pub type Pusher = rules::Pusher<AccountId, TeamId>;

impl rules::RefChange for RefUpdate {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_created(&self) -> bool {
        RefUpdate::is_created(self)
    }

    fn is_deleted(&self) -> bool {
        RefUpdate::is_deleted(self)
    }
}

/// Required contexts are parsed by the rule management through this.
impl std::str::FromStr for StatusContext {
    type Err = StatusError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// Who is pushing.
#[derive(Debug, Clone, Copy)]
pub struct PushActor<'a> {
    /// The signed-in user; `None` for deploy keys.
    pub user: Option<&'a User>,
    /// Whether the push is made with a deploy key.
    pub deploy_key: bool,
    /// The pusher's access to the repository, after the token scope.
    pub access: RepoAccess,
}

/// A stored rule with what its pages show.
#[derive(Debug, Clone)]
pub struct BranchProtection {
    pub id: ProtectionId,
    pub repo_id: RepoId,
    pub rule: ProtectionRule,
    /// The accounts behind `rule.allowed_users`, by name.
    pub users: Vec<Account>,
    /// The teams behind `rule.allowed_teams`, by name.
    pub teams: Vec<Team>,
    pub created_by: Option<AccountId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
