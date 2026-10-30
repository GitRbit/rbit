//! The Actions policies of a repository and of its owner, in GitHub's terms (the
//! `actions/permissions` REST endpoints): which repositories run workflows, which actions
//! they may use, the workflow token's default permission, whether it may approve pull
//! requests, and which outside contributors wait for approval.
//!
//! An owner's (a user's or an organisation's) policies are ceilings: a repository's own
//! setting counts only as far as its owner allows, and [`OwnerActionsSettings::default`]
//! caps nothing. [`effective`] combines the two.

use super::DefaultTokenPermission;

/// Which actions workflows may use besides rbit's built-ins (`actions/checkout`, the cache
/// and artifact actions) and the workspace's own (`./path`): GitHub's `allowed_actions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AllowedActions {
    #[default]
    All,
    /// None: actions of the repository itself only.
    LocalOnly,
    /// Those [`SelectedActions`] allows.
    Selected,
}

impl AllowedActions {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::LocalOnly => "local_only",
            Self::Selected => "selected",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::All, Self::LocalOnly, Self::Selected].into_iter().find(|allowed| allowed.as_str() == value)
    }
}

/// The actions an [`AllowedActions::Selected`] policy allows: GitHub's `selected-actions`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectedActions {
    /// Actions of GitHub's own `actions` and `github` organisations.
    pub github_owned_allowed: bool,
    /// `owner/repo[/path]@ref` patterns where `*` matches anything (`octo-org/*`,
    /// `octo-org/deploy@*`), and `docker://image` patterns.
    pub patterns_allowed: Vec<String>,
}

impl Default for SelectedActions {
    fn default() -> Self {
        Self { github_owned_allowed: true, patterns_allowed: Vec::new() }
    }
}

/// The most patterns one policy holds, and the longest pattern.
pub const MAX_ACTION_PATTERNS: usize = 256;
pub const MAX_ACTION_PATTERN_BYTES: usize = 512;

/// Which pull request authors without write access wait until a writer approves their run:
/// GitHub's `approval_policy`, strictest last. rbit cannot tell who is new to the platform
/// the way GitHub does, so [`Self::FirstTimeContributorsNewToGithub`] waits exactly like
/// [`Self::FirstTimeContributors`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum ApprovalPolicy {
    FirstTimeContributorsNewToGithub,
    /// Authors nobody vouched for: neither trusted by the repository nor the author of a
    /// pull request merged into it.
    #[default]
    FirstTimeContributors,
    /// Every author without write access the repository has not trusted explicitly; a
    /// merged pull request vouches for nobody.
    AllExternalContributors,
}

impl ApprovalPolicy {
    pub const ALL: [Self; 3] = [Self::FirstTimeContributorsNewToGithub, Self::FirstTimeContributors, Self::AllExternalContributors];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FirstTimeContributorsNewToGithub => "first_time_contributors_new_to_github",
            Self::FirstTimeContributors => "first_time_contributors",
            Self::AllExternalContributors => "all_external_contributors",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|policy| policy.as_str() == value)
    }

    /// Whether a merged pull request vouches for its author.
    pub const fn merged_pull_vouches(self) -> bool {
        !matches!(self, Self::AllExternalContributors)
    }
}

/// Which of an owner's repositories may run workflows: GitHub's `enabled_repositories`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EnabledRepositories {
    #[default]
    All,
    None,
    /// Those the owner selected.
    Selected,
}

impl EnabledRepositories {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::None => "none",
            Self::Selected => "selected",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::All, Self::None, Self::Selected].into_iter().find(|enabled| enabled.as_str() == value)
    }
}

/// The policies a repository or an owner sets, one value each.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActionsPolicies {
    pub allowed_actions: AllowedActions,
    /// Kept while `allowed_actions` is something else, as GitHub keeps it.
    pub selected_actions: SelectedActions,
    pub token_permission: DefaultTokenPermission,
    pub can_approve_reviews: bool,
    pub approval_policy: ApprovalPolicy,
}

impl ActionsPolicies {
    /// A repository's until it changes them: every action, a read token that approves
    /// nothing, first-time contributors wait.
    pub fn repository_default() -> Self {
        Self {
            allowed_actions: AllowedActions::All,
            selected_actions: SelectedActions::default(),
            token_permission: DefaultTokenPermission::Read,
            can_approve_reviews: false,
            approval_policy: ApprovalPolicy::FirstTimeContributors,
        }
    }

    /// An owner's until it changes them: ceilings that cap nothing.
    pub fn owner_default() -> Self {
        Self {
            allowed_actions: AllowedActions::All,
            selected_actions: SelectedActions::default(),
            token_permission: DefaultTokenPermission::Write,
            can_approve_reviews: true,
            approval_policy: ApprovalPolicy::FirstTimeContributorsNewToGithub,
        }
    }

    /// The settings `self` (a repository's) asks for beyond `ceiling` (its owner's), by
    /// GitHub's field names; empty when the owner allows all of them. Selected actions
    /// never exceed a ceiling: both lists apply.
    pub fn exceeding(&self, ceiling: &Self) -> Vec<&'static str> {
        let rank = |allowed: AllowedActions| match allowed {
            AllowedActions::LocalOnly => 0,
            AllowedActions::Selected => 1,
            AllowedActions::All => 2,
        };
        let mut fields = Vec::new();
        if rank(self.allowed_actions) > rank(ceiling.allowed_actions) {
            fields.push("allowed_actions");
        }
        if self.token_permission == DefaultTokenPermission::Write && ceiling.token_permission == DefaultTokenPermission::Read {
            fields.push("default_workflow_permissions");
        }
        if self.can_approve_reviews && !ceiling.can_approve_reviews {
            fields.push("can_approve_pull_request_reviews");
        }
        if self.approval_policy < ceiling.approval_policy {
            fields.push("approval_policy");
        }
        fields
    }
}

/// An owner's Actions settings: which repositories run workflows, and its ceilings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerActionsSettings {
    pub enabled_repositories: EnabledRepositories,
    pub policies: ActionsPolicies,
}

impl Default for OwnerActionsSettings {
    fn default() -> Self {
        Self { enabled_repositories: EnabledRepositories::All, policies: ActionsPolicies::owner_default() }
    }
}

/// The policies that apply to a repository's runs: its own, capped by its owner's. Both
/// actions policies stay, since an action must pass each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectivePolicies {
    /// The repository's, then the owner's.
    pub actions: Vec<(AllowedActions, SelectedActions)>,
    pub token_permission: DefaultTokenPermission,
    pub can_approve_reviews: bool,
    pub approval_policy: ApprovalPolicy,
}

/// What applies to a repository whose own policies are `repo` and whose owner's are `owner`.
pub fn effective(repo: &ActionsPolicies, owner: &ActionsPolicies) -> EffectivePolicies {
    let token_permission =
        if owner.token_permission == DefaultTokenPermission::Read { DefaultTokenPermission::Read } else { repo.token_permission };
    EffectivePolicies {
        actions: vec![(repo.allowed_actions, repo.selected_actions.clone()), (owner.allowed_actions, owner.selected_actions.clone())],
        token_permission,
        can_approve_reviews: repo.can_approve_reviews && owner.can_approve_reviews,
        approval_policy: repo.approval_policy.max(owner.approval_policy),
    }
}

/// Why a pattern of [`SelectedActions::patterns_allowed`] is refused, or `None`.
pub fn invalid_pattern(pattern: &str) -> Option<&'static str> {
    if pattern.is_empty() || pattern.len() > MAX_ACTION_PATTERN_BYTES {
        Some("a pattern is empty or too long")
    } else if pattern.chars().any(|c| c.is_whitespace() || c.is_control()) {
        Some("a pattern contains whitespace")
    } else if !pattern.starts_with("docker://") && !pattern.contains('/') && pattern != "*" {
        Some("a pattern names `owner/repo`, `owner/*` or `docker://image`")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_githubs() {
        for policy in ApprovalPolicy::ALL {
            assert_eq!(ApprovalPolicy::parse(policy.as_str()), Some(policy));
        }
        assert_eq!(AllowedActions::parse("local_only"), Some(AllowedActions::LocalOnly));
        assert_eq!(EnabledRepositories::parse("selected"), Some(EnabledRepositories::Selected));
        assert_eq!(AllowedActions::parse("All"), None);
    }

    #[test]
    fn owners_cap_their_repositories() {
        let repo = ActionsPolicies::repository_default();
        let mut owner = ActionsPolicies::owner_default();
        assert_eq!(repo.exceeding(&owner), Vec::<&str>::new(), "defaults fit");
        assert_eq!(effective(&repo, &owner).token_permission, DefaultTokenPermission::Read);

        let wide = ActionsPolicies {
            token_permission: DefaultTokenPermission::Write,
            can_approve_reviews: true,
            approval_policy: ApprovalPolicy::FirstTimeContributorsNewToGithub,
            ..repo.clone()
        };
        assert_eq!(effective(&wide, &owner).token_permission, DefaultTokenPermission::Write);
        owner.allowed_actions = AllowedActions::Selected;
        assert_eq!(wide.exceeding(&owner), ["allowed_actions"]);
        owner.allowed_actions = AllowedActions::All;
        owner.token_permission = DefaultTokenPermission::Read;
        assert_eq!(wide.exceeding(&owner), ["default_workflow_permissions"]);
        let capped = effective(&wide, &owner);
        assert_eq!(capped.token_permission, DefaultTokenPermission::Read, "a stored setting past the ceiling is capped");
        owner.token_permission = DefaultTokenPermission::Write;
        owner.can_approve_reviews = false;
        assert_eq!(wide.exceeding(&owner), ["can_approve_pull_request_reviews"]);
        assert!(!effective(&wide, &owner).can_approve_reviews);
        owner.can_approve_reviews = true;
        owner.approval_policy = ApprovalPolicy::AllExternalContributors;
        assert_eq!(wide.exceeding(&owner), ["approval_policy"]);
        assert_eq!(effective(&wide, &owner).approval_policy, ApprovalPolicy::AllExternalContributors, "the stricter applies");
    }

    #[test]
    fn patterns() {
        for good in ["actions/*", "octo-org/deploy@v2", "octo/repo/path@*", "docker://alpine:3", "*"] {
            assert_eq!(invalid_pattern(good), None, "{good}");
        }
        for bad in ["", "no slash", "plain", "a/b\n"] {
            assert!(invalid_pattern(bad).is_some(), "{bad:?}");
        }
    }
}
