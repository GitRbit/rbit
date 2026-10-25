//! Branch protection: rules per branch pattern and the pure decisions made from them —
//! which ref updates a push may make ([`decide_push`] → [`PushVerdict`]) and what merging
//! a pull request into a branch requires ([`merge_rules`] → [`MergeRules`]).
//!
//! Milestone 2 workstream B (branch protection) owns this file. The push-gate types are
//! the shared contract of `docs/design/milestone-2b.md` §3.
//!
//! Rules:
//! * A rule names a [`BranchPattern`]: an exact branch name or a glob (`*` within one
//!   path segment, `**` across segments, `?` one character other than `/`). Rules only
//!   concern `refs/heads/*`. On Windows and macOS, case aliases share protection because
//!   repositories commonly live on case-insensitive filesystems.
//! * Every matching rule applies and the strictest value wins: any denial denies, the
//!   highest approval count counts, and pusher restrictions intersect.
//! * `allow_admin_bypass` lets repository administrators pass *that* rule; a matching rule
//!   without it still applies to them. Nothing bypasses the reserved `refs/pull/`
//!   namespace.
//! * Refusal texts never say who is allowed.
//! * `require_status_checks` (milestone 3, S) adds the rule's required contexts to what
//!   merging needs: every required context's latest status on the head must be `success`.
//!   It does not affect pushes.

use std::{fmt, str::FromStr};

use crate::{
    Account, AccountId, RefUpdate, RepoAccess, RepoId, Timestamp, User,
    membership::{Team, TeamId},
    status::StatusContext,
};

/// Longest pattern, in bytes.
pub const MAX_PATTERN_LEN: usize = 255;
/// Most rules per repository.
pub const MAX_RULES: usize = 50;
/// Most allowed users, and most allowed teams, per rule.
pub const MAX_ALLOWED: usize = 50;
/// Most approvals a rule may require.
pub const MAX_REQUIRED_APPROVALS: u8 = 10;

/// The namespace rbit keeps pull request heads in; clients may never update it.
pub const PULL_REFS: &str = "refs/pull/";

/// Refusal reasons, as the client shows them (`! [remote rejected] main -> main (…)`).
pub mod reasons {
    pub const PULL_REFS: &str = "refs/pull/ is managed by rbit";
    pub const NAMESPACE_CASE: &str = "use the canonical refs/heads/ namespace";
    pub const DELETION: &str = "protected branch: deleting it is not allowed";
    pub const RESTRICTED: &str = "protected branch: you are not allowed to push to it";
    pub const PULL_REQUEST: &str = "protected branch: changes must go through a pull request";
    pub const NO_WRITE: &str = "you are not allowed to push to this repository";
}

/// One ref update a push may not make, and why. `reason` is shown to the client
/// (`! [remote rejected] main -> main (<reason>)`) and never lists who is allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The full ref name, e.g. `refs/heads/main`.
    pub name: String,
    pub reason: String,
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

/// What the push gate decided before git sees the push.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PushVerdict {
    /// Every refused ref; when not empty, the whole push is refused.
    pub refusals: Vec<Refusal>,
    /// Run `git receive-pack` with `receive.denyNonFastForwards`: some update of an
    /// existing branch is protected against force pushes for this pusher.
    pub deny_non_fast_forwards: bool,
}

impl PushVerdict {
    /// Everything is allowed.
    pub fn allow() -> Self {
        Self::default()
    }

    /// Every update refused for the same `reason`, whatever the rules say (a push to an
    /// archived repository).
    pub fn refuse_all(updates: &[RefUpdate], reason: &str) -> Self {
        let refusals = updates.iter().map(|update| Refusal { name: update.name.clone(), reason: reason.to_owned() }).collect();
        Self { refusals, deny_non_fast_forwards: false }
    }

    pub fn is_refused(&self) -> bool {
        !self.refusals.is_empty()
    }
}

/// What merging a pull request into a branch requires of the merger.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "independent rule flags")]
pub struct MergeRules {
    /// The patterns of the rules that match the branch, for display.
    pub patterns: Vec<String>,
    pub require_pull_request: bool,
    pub required_approvals: u8,
    pub dismiss_stale_approvals: bool,
    pub block_on_changes_requested: bool,
    /// `restrict_pushers` allows the merger (always true without restrictions).
    pub merger_may_push: bool,
    /// The merger is an administrator and every matching rule allows administrators to
    /// bypass it.
    pub merger_may_bypass: bool,
    /// Some matching rule requires status checks (milestone 3, S).
    pub require_status_checks: bool,
    /// The contexts those rules require, sorted and without duplicates.
    pub required_contexts: Vec<StatusContext>,
}

impl MergeRules {
    /// No rule matches the branch.
    pub fn unprotected() -> Self {
        Self {
            patterns: Vec::new(),
            require_pull_request: false,
            required_approvals: 0,
            dismiss_stale_approvals: false,
            block_on_changes_requested: false,
            merger_may_push: true,
            merger_may_bypass: false,
            require_status_checks: false,
            required_contexts: Vec::new(),
        }
    }

    pub fn is_protected(&self) -> bool {
        !self.patterns.is_empty()
    }
}

// ------------------------------------------------------------------ patterns

/// Why a pattern was rejected. The message is shown to the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatternError {
    #[error("enter a branch name or pattern")]
    Empty,
    #[error("the pattern must be at most {MAX_PATTERN_LEN} bytes")]
    TooLong,
    #[error("the pattern may not contain {0:?}")]
    Character(char),
    #[error("the pattern may not contain spaces or control characters")]
    Whitespace,
    #[error("the pattern may not contain {0:?}")]
    Sequence(&'static str),
    #[error("the pattern may not start or end with \"/\" or end with \".\"")]
    Edge,
    #[error("no part of the pattern may start with \".\" or end with \".lock\"")]
    Segment,
    #[error("use * or ** (not three or more stars in a row)")]
    Stars,
}

/// A branch name or glob a rule applies to (without `refs/heads/`). Validated: at most
/// [`MAX_PATTERN_LEN`] bytes of the characters a branch name may contain, plus `*` and `?`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BranchPattern(String);

/// One element of a parsed pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Literal(char),
    /// `?`: one character other than `/`.
    One,
    /// `*`: any run of characters other than `/`.
    Star,
    /// `**`: any run of characters, `/` included. `**/` also matches nothing at all, so
    /// `**/main` matches `main` too.
    Globstar,
}

impl BranchPattern {
    pub fn parse(value: &str) -> Result<Self, PatternError> {
        let value = value.trim();
        if value.is_empty() {
            return Err(PatternError::Empty);
        }
        if value.len() > MAX_PATTERN_LEN {
            return Err(PatternError::TooLong);
        }
        if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(PatternError::Whitespace);
        }
        if let Some(c) = value.chars().find(|c| matches!(c, '~' | '^' | ':' | '[' | '\\')) {
            return Err(PatternError::Character(c));
        }
        for sequence in ["..", "@{", "//"] {
            if value.contains(sequence) {
                return Err(PatternError::Sequence(sequence));
            }
        }
        if value == "@" {
            return Err(PatternError::Character('@'));
        }
        if value.starts_with('/') || value.ends_with('/') || value.ends_with('.') {
            return Err(PatternError::Edge);
        }
        if value.split('/').any(|segment| segment.starts_with('.') || segment.strip_suffix(".lock").is_some()) {
            return Err(PatternError::Segment);
        }
        if value.contains("***") {
            return Err(PatternError::Stars);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is a glob rather than one exact branch name.
    pub fn is_glob(&self) -> bool {
        self.0.contains(['*', '?'])
    }

    fn tokens(&self) -> Vec<Token> {
        let mut tokens = Vec::with_capacity(self.0.len());
        let mut chars = self.0.chars().peekable();
        while let Some(c) = chars.next() {
            tokens.push(match c {
                '*' if chars.peek() == Some(&'*') => {
                    chars.next();
                    Token::Globstar
                }
                '*' => Token::Star,
                '?' => Token::One,
                other => Token::Literal(other),
            });
        }
        tokens
    }

    /// Whether `branch` (a name without `refs/heads/`) matches. Case aliases match on
    /// Windows and macOS so filesystem aliases cannot escape protection.
    pub fn matches(&self, branch: &str) -> bool {
        if !self.is_glob() {
            return ref_names_equal(&self.0, branch);
        }
        let tokens = self.tokens();
        let text: Vec<char> = branch.chars().collect();
        // matched[t][s]: tokens[t..] match text[s..]. Filled backwards; O(tokens × text).
        let width = text.len() + 1;
        let mut matched = vec![false; (tokens.len() + 1) * width];
        matched[tokens.len() * width + text.len()] = true;
        for t in (0..tokens.len()).rev() {
            for s in (0..=text.len()).rev() {
                let next = |t: usize, s: usize| matched[t * width + s];
                let here = match tokens[t] {
                    Token::Literal(c) => s < text.len() && ref_char_equal(text[s], c) && next(t + 1, s + 1),
                    Token::One => s < text.len() && text[s] != '/' && next(t + 1, s + 1),
                    Token::Star => next(t + 1, s) || (s < text.len() && text[s] != '/' && next(t, s + 1)),
                    Token::Globstar => {
                        next(t + 1, s)
                            || (s < text.len() && next(t, s + 1))
                            || (tokens.get(t + 1) == Some(&Token::Literal('/')) && next(t + 2, s))
                    }
                };
                matched[t * width + s] = here;
            }
        }
        matched[0]
    }
}

/// Whether two ref names can identify the same filesystem entry. Matching is
/// conservatively case-insensitive on Windows and macOS, including Unicode case
/// mappings; the original spelling is still used for every Git operation.
fn ref_names_equal(left: &str, right: &str) -> bool {
    if !cfg!(any(windows, target_os = "macos")) {
        return left == right;
    }
    let mut right = right.chars();
    left.chars().all(|left| right.next().is_some_and(|right| ref_char_equal(left, right))) && right.next().is_none()
}

fn ref_char_equal(left: char, right: char) -> bool {
    left == right
        || (cfg!(any(windows, target_os = "macos"))
            && (left.to_lowercase().eq(right.to_lowercase()) || left.to_uppercase().eq(right.to_uppercase())))
}

fn namespace_refusal(name: &str) -> Option<&'static str> {
    let (refs, rest) = name.split_once('/')?;
    let (namespace, _) = rest.split_once('/')?;
    if !ref_names_equal(refs, "refs") {
        return None;
    }
    if ref_names_equal(namespace, "pull") {
        return Some(reasons::PULL_REFS);
    }
    // Git's receive.denyNonFastForwards only recognises the exact refs/heads/
    // spelling, even when its backing filesystem treats another spelling as equal.
    if ref_names_equal(namespace, "heads") && (refs != "refs" || namespace != "heads") {
        return Some(reasons::NAMESPACE_CASE);
    }
    None
}

impl FromStr for BranchPattern {
    type Err = PatternError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Display for BranchPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ------------------------------------------------------------------ rules

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
pub struct ProtectionRule {
    pub pattern: BranchPattern,
    pub settings: RuleSettings,
    /// With `restrict_pushers`: users who may push.
    pub allowed_users: Vec<AccountId>,
    /// With `restrict_pushers`: teams whose members may push.
    pub allowed_teams: Vec<TeamId>,
    /// With `require_status_checks`: the contexts that must pass.
    pub required_contexts: Vec<StatusContext>,
}

impl ProtectionRule {
    pub fn matches(&self, branch: &str) -> bool {
        self.pattern.matches(branch)
    }
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

// ------------------------------------------------------------------ decisions

/// The pusher as the decisions see them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pusher {
    DeployKey {
        access: RepoAccess,
    },
    /// A user, with the teams they are on (at least those the rules name).
    User {
        id: AccountId,
        access: RepoAccess,
        teams: Vec<TeamId>,
    },
}

impl Pusher {
    fn access(&self) -> RepoAccess {
        match self {
            Self::DeployKey { access } | Self::User { access, .. } => *access,
        }
    }

    /// Repository administrators (deploy keys never are).
    fn is_admin(&self) -> bool {
        matches!(self, Self::User { access, .. } if access.can_admin())
    }

    /// Whether a restricting `rule` lets this pusher push (deploy keys never).
    fn allowed_by(&self, rule: &ProtectionRule) -> bool {
        match self {
            Self::DeployKey { .. } => false,
            Self::User { id, teams, .. } => rule.allowed_users.contains(id) || teams.iter().any(|team| rule.allowed_teams.contains(team)),
        }
    }
}

/// A rule an administrator passed only thanks to `allow_admin_bypass`: worth an audit
/// entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bypass {
    /// The full ref name.
    pub name: String,
    /// What would have happened without the bypass: a refusal reason, or force-push
    /// protection.
    pub instead: String,
}

/// [`decide_push`]'s result: the verdict and the bypasses that changed it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PushDecision {
    pub verdict: PushVerdict,
    pub bypasses: Vec<Bypass>,
}

/// What one ref update would get, from the rules considered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Outcome {
    /// The refusal reason of highest priority.
    refusal: Option<&'static str>,
    deny_non_fast_forwards: bool,
}

impl Outcome {
    fn refuse(&mut self, reason: &'static str) {
        let priority = |reason| [reasons::DELETION, reasons::RESTRICTED, reasons::PULL_REQUEST].iter().position(|r| *r == reason);
        if self.refusal.is_none_or(|current| priority(reason) < priority(current)) {
            self.refusal = Some(reason);
        }
    }

    fn description(self) -> String {
        match self.refusal {
            Some(reason) => format!("refused: {reason}"),
            None if self.deny_non_fast_forwards => "force pushes denied".to_owned(),
            None => "allowed".to_owned(),
        }
    }
}

/// What `rule` does to `update` (of a branch it matches) pushed by `pusher`.
fn apply(rule: &ProtectionRule, pusher: &Pusher, update: &RefUpdate, outcome: &mut Outcome) {
    let settings = &rule.settings;
    if update.is_deleted() && settings.deny_deletion {
        outcome.refuse(reasons::DELETION);
    }
    if settings.restrict_pushers && !pusher.allowed_by(rule) {
        outcome.refuse(reasons::RESTRICTED);
    }
    let updates_existing = !update.is_created() && !update.is_deleted();
    if updates_existing && settings.require_pull_request {
        outcome.refuse(reasons::PULL_REQUEST);
    }
    if updates_existing && settings.deny_force_push {
        outcome.deny_non_fast_forwards = true;
    }
}

/// Decides the ref updates a client asked for, before any object arrives.
///
/// * `refs/pull/*`: always refused ([`reasons::PULL_REFS`]);
/// * a pusher without write access: everything refused (the transports check first;
///   this is defence in depth);
/// * branches: every matching rule applies (deletion, then restriction, then "pull
///   request" decide the reason shown); updates of an existing branch protected
///   against force pushes set `deny_non_fast_forwards`;
/// * other refs (tags, notes): allowed.
///
/// An administrator passes each rule with `allow_admin_bypass`; where that changed the
/// outcome for a ref, a [`Bypass`] is reported.
pub fn decide_push(rules: &[ProtectionRule], pusher: &Pusher, updates: &[RefUpdate]) -> PushDecision {
    let mut decision = PushDecision::default();
    for update in updates {
        if let Some(reason) = namespace_refusal(&update.name) {
            decision.verdict.refusals.push(Refusal { name: update.name.clone(), reason: reason.to_owned() });
            continue;
        }
        if !pusher.access().can_write() {
            decision.verdict.refusals.push(Refusal { name: update.name.clone(), reason: reasons::NO_WRITE.to_owned() });
            continue;
        }
        let Some(branch) = update.branch() else { continue };
        let (mut enforced, mut strict) = (Outcome::default(), Outcome::default());
        for rule in rules.iter().filter(|rule| rule.matches(branch)) {
            apply(rule, pusher, update, &mut strict);
            if !(rule.settings.allow_admin_bypass && pusher.is_admin()) {
                apply(rule, pusher, update, &mut enforced);
            }
        }
        if enforced != strict {
            decision.bypasses.push(Bypass { name: update.name.clone(), instead: strict.description() });
        }
        if let Some(reason) = enforced.refusal {
            decision.verdict.refusals.push(Refusal { name: update.name.clone(), reason: reason.to_owned() });
        } else if enforced.deny_non_fast_forwards {
            decision.verdict.deny_non_fast_forwards = true;
        }
    }
    decision
}

/// What merging into `branch` requires of `merger`, from the rules matching it. Approval
/// options count only on rules that require a pull request.
pub fn merge_rules(rules: &[ProtectionRule], merger: &Pusher, branch: &str) -> MergeRules {
    let matching: Vec<&ProtectionRule> = rules.iter().filter(|rule| rule.matches(branch)).collect();
    if matching.is_empty() {
        return MergeRules::unprotected();
    }
    let with_pull_requests = || matching.iter().filter(|rule| rule.settings.require_pull_request);
    let with_checks = || matching.iter().filter(|rule| rule.settings.require_status_checks);
    let mut required_contexts: Vec<StatusContext> = with_checks().flat_map(|rule| rule.required_contexts.iter().cloned()).collect();
    required_contexts.sort();
    required_contexts.dedup();
    MergeRules {
        patterns: matching.iter().map(|rule| rule.pattern.to_string()).collect(),
        require_pull_request: with_pull_requests().next().is_some(),
        required_approvals: matching.iter().map(|rule| rule.settings.required_approvals).max().unwrap_or(0),
        dismiss_stale_approvals: with_pull_requests().any(|rule| rule.settings.dismiss_stale_approvals),
        block_on_changes_requested: with_pull_requests().any(|rule| rule.settings.block_on_changes_requested),
        merger_may_push: matching.iter().all(|rule| !rule.settings.restrict_pushers || merger.allowed_by(rule)),
        merger_may_bypass: merger.is_admin() && matching.iter().all(|rule| rule.settings.allow_admin_bypass),
        require_status_checks: with_checks().next().is_some(),
        required_contexts,
    }
}

/// Whether `pusher` may delete `branch` (the same decision as a push deleting it).
pub fn may_delete(rules: &[ProtectionRule], pusher: &Pusher, branch: &str) -> bool {
    let update = RefUpdate { name: format!("refs/heads/{branch}"), before: Some(String::new()), after: None };
    !decide_push(rules, pusher, &[update]).verdict.is_refused()
}

/// Whether any rule matches `branch`.
pub fn is_protected(rules: &[ProtectionRule], branch: &str) -> bool {
    rules.iter().any(|rule| rule.matches(branch))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(value: &str) -> BranchPattern {
        value.parse().unwrap()
    }

    fn context(value: &str) -> StatusContext {
        StatusContext::parse(value).unwrap()
    }

    #[test]
    fn patterns_are_validated() {
        for valid in ["main", "release/*", "release/**", "**/hotfix", "v?.x", "feature/*-wip", "a.b/c_d", "日本"] {
            assert_eq!(pattern(valid).as_str(), valid);
        }
        assert_eq!(pattern("  main ").as_str(), "main", "trimmed");
        let cases = [
            ("", PatternError::Empty),
            ("a b", PatternError::Whitespace),
            ("a\tb", PatternError::Whitespace),
            ("a~b", PatternError::Character('~')),
            ("a^b", PatternError::Character('^')),
            ("a:b", PatternError::Character(':')),
            ("[ab]", PatternError::Character('[')),
            ("a\\b", PatternError::Character('\\')),
            ("a..b", PatternError::Sequence("..")),
            ("a@{b", PatternError::Sequence("@{")),
            ("a//b", PatternError::Sequence("//")),
            ("@", PatternError::Character('@')),
            ("/main", PatternError::Edge),
            ("main/", PatternError::Edge),
            ("main.", PatternError::Edge),
            (".hidden", PatternError::Segment),
            ("a/.b", PatternError::Segment),
            ("x.lock", PatternError::Segment),
            ("a/***", PatternError::Stars),
        ];
        for (value, error) in cases {
            assert_eq!(BranchPattern::parse(value), Err(error), "{value:?}");
        }
        assert_eq!(BranchPattern::parse(&"a".repeat(256)), Err(PatternError::TooLong));
        assert!(BranchPattern::parse(&"a".repeat(255)).is_ok());
        assert!(!pattern("main").is_glob() && pattern("m*").is_glob() && pattern("m?").is_glob());
    }

    #[test]
    fn pattern_matching_table() {
        let table: &[(&str, &str, bool)] = &[
            ("main", "main", true),
            ("main", "Main", cfg!(any(windows, target_os = "macos"))),
            ("main", "main2", false),
            ("main", "x/main", false),
            ("*", "main", true),
            ("*", "release/1.0", false),
            ("release/*", "release/1.0", true),
            ("release/*", "release/1.0/hotfix", false),
            ("release/*", "release", false),
            ("release/*", "Release/1.0", cfg!(any(windows, target_os = "macos"))),
            ("release/**", "release/1.0/hotfix", true),
            ("release/**", "release/1.0", true),
            ("release/**", "release", false),
            ("**", "a/b/c", true),
            ("**/main", "main", true),
            ("**/main", "a/b/main", true),
            ("**/main", "a/mainx", false),
            ("a/**/b", "a/b", true),
            ("a/**/b", "a/x/y/b", true),
            ("a/**/b", "ab", false),
            ("v?", "v1", true),
            ("v?", "v10", false),
            ("v?", "v/", false),
            ("?", "", false),
            ("*-wip", "feature-wip", true),
            ("*-wip", "feature/x-wip", false),
            ("f*e*x", "feature-x", true),
            ("f*e*x", "feature-y", false),
            ("**-wip", "a/b-wip", true),
            ("日*", "日本", true),
        ];
        for (glob, branch, expected) in table {
            assert_eq!(pattern(glob).matches(branch), *expected, "{glob:?} vs {branch:?}");
        }
    }

    #[test]
    fn matching_long_names_stays_fast() {
        let glob = pattern("*a*a*a*a*a*a*a*a*b");
        assert!(!glob.matches(&"a".repeat(250)));
    }

    #[test]
    fn settings_validation_and_defaults() {
        let defaults = RuleSettings::default();
        assert!(defaults.deny_force_push && defaults.deny_deletion && defaults.block_on_changes_requested);
        assert!(!defaults.require_pull_request && !defaults.restrict_pushers && !defaults.allow_admin_bypass);
        assert_eq!(defaults.validate(), Ok(()));
        let approvals = RuleSettings { required_approvals: 2, ..defaults };
        assert_eq!(approvals.validate(), Err(RuleError::ApprovalsWithoutPullRequest));
        assert_eq!(RuleSettings { require_pull_request: true, ..approvals }.validate(), Ok(()));
        let too_many = RuleSettings { require_pull_request: true, required_approvals: 11, ..defaults };
        assert_eq!(too_many.validate(), Err(RuleError::TooManyApprovals));
    }

    // ---------------------------------------------------------- the decision matrix

    const ALICE: AccountId = AccountId(1);
    const LISTED: AccountId = AccountId(2);
    const TEAM: TeamId = TeamId(7);
    const OTHER_TEAM: TeamId = TeamId(8);

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Actor {
        DeployKey,
        Reader,
        Writer,
        Admin,
        ListedUser,
        TeamMember,
    }

    const ACTORS: [Actor; 6] = [Actor::DeployKey, Actor::Reader, Actor::Writer, Actor::Admin, Actor::ListedUser, Actor::TeamMember];

    impl Actor {
        fn pusher(self) -> Pusher {
            match self {
                Self::DeployKey => Pusher::DeployKey { access: RepoAccess::Write },
                Self::Reader => Pusher::User { id: ALICE, access: RepoAccess::Read, teams: vec![] },
                Self::Writer => Pusher::User { id: ALICE, access: RepoAccess::Write, teams: vec![OTHER_TEAM] },
                Self::Admin => Pusher::User { id: ALICE, access: RepoAccess::Admin, teams: vec![] },
                Self::ListedUser => Pusher::User { id: LISTED, access: RepoAccess::Write, teams: vec![] },
                Self::TeamMember => Pusher::User { id: ALICE, access: RepoAccess::Write, teams: vec![OTHER_TEAM, TEAM] },
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Create,
        Update,
        Delete,
    }

    fn update(name: &str, kind: Kind) -> RefUpdate {
        let id = "1".repeat(40);
        let (before, after) = match kind {
            Kind::Create => (None, Some(id)),
            Kind::Update => (Some(id.clone()), Some(id)),
            Kind::Delete => (Some(id), None),
        };
        RefUpdate { name: name.to_owned(), before, after }
    }

    fn rule(glob: &str, settings: RuleSettings) -> ProtectionRule {
        ProtectionRule {
            pattern: pattern(glob),
            settings,
            allowed_users: vec![LISTED],
            allowed_teams: vec![TEAM],
            required_contexts: vec![context("ci")],
        }
    }

    /// The reference table, written independently of [`decide_push`]: what one rule with
    /// these flags does to one update of a branch it matches. `None`: allowed.
    fn expected(flags: [bool; 5], actor: Actor, kind: Kind) -> (Option<&'static str>, bool) {
        let [force, deletion, pull_request, restrict, bypass] = flags;
        if actor == Actor::Reader {
            return (Some(reasons::NO_WRITE), false);
        }
        if bypass && actor == Actor::Admin {
            return (None, false);
        }
        let allowed = matches!(actor, Actor::ListedUser | Actor::TeamMember);
        match kind {
            Kind::Delete if deletion => (Some(reasons::DELETION), false),
            _ if restrict && !allowed => (Some(reasons::RESTRICTED), false),
            Kind::Update if pull_request => (Some(reasons::PULL_REQUEST), false),
            Kind::Update => (None, force),
            Kind::Create | Kind::Delete => (None, false),
        }
    }

    #[test]
    fn every_flag_actor_and_update_kind() {
        let mut cases = 0;
        for bits in 0..32_u8 {
            let flags = [0, 1, 2, 3, 4].map(|bit| bits & (1 << bit) != 0);
            let settings = RuleSettings {
                deny_force_push: flags[0],
                deny_deletion: flags[1],
                require_pull_request: flags[2],
                restrict_pushers: flags[3],
                allow_admin_bypass: flags[4],
                ..RuleSettings::default()
            };
            let rules = [rule("main", settings)];
            for actor in ACTORS {
                for kind in [Kind::Create, Kind::Update, Kind::Delete] {
                    let decision = decide_push(&rules, &actor.pusher(), &[update("refs/heads/main", kind)]);
                    let (reason, deny_nff) = expected(flags, actor, kind);
                    let got = decision.verdict.refusals.first().map(|refusal| refusal.reason.as_str());
                    assert_eq!((got, decision.verdict.deny_non_fast_forwards), (reason, deny_nff), "{flags:?} {actor:?} {kind:?}");

                    // A bypass is reported exactly when it changed the outcome.
                    let strict = expected([flags[0], flags[1], flags[2], flags[3], false], actor, kind);
                    assert_eq!(decision.bypasses.len(), usize::from(strict != (reason, deny_nff)), "{flags:?} {actor:?} {kind:?}");

                    // Unprotected branches and tags are never touched by the rule.
                    for name in ["refs/heads/other", "refs/tags/main"] {
                        let free = decide_push(&rules, &actor.pusher(), &[update(name, kind)]);
                        let expected_free = if actor == Actor::Reader { Some(reasons::NO_WRITE) } else { None };
                        assert_eq!(free.verdict.refusals.first().map(|r| r.reason.as_str()), expected_free);
                        assert!(!free.verdict.deny_non_fast_forwards && free.bypasses.is_empty());
                    }
                    // The reserved namespace is refused for everyone, whatever the rules.
                    let pull = decide_push(&rules, &actor.pusher(), &[update("refs/pull/1/head", kind)]);
                    assert_eq!(pull.verdict.refusals[0].reason, reasons::PULL_REFS);
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 32 * ACTORS.len() * 3);
    }

    #[test]
    fn matching_rules_combine_strictly() {
        let admin = Actor::Admin.pusher();
        let writer = Actor::Writer.pusher();
        let lenient = RuleSettings { deny_force_push: false, deny_deletion: false, ..RuleSettings::default() };
        let rules =
            [rule("release/**", RuleSettings { allow_admin_bypass: true, ..RuleSettings::default() }), rule("release/1.*", lenient)];
        // The force-push protection of the first rule applies to writers.
        let decision = decide_push(&rules, &writer, &[update("refs/heads/release/1.0", Kind::Update)]);
        assert!(decision.verdict.deny_non_fast_forwards && !decision.verdict.is_refused());
        // The admin bypasses the first rule; the second protects nothing.
        let decision = decide_push(&rules, &admin, &[update("refs/heads/release/1.0", Kind::Delete)]);
        assert!(!decision.verdict.is_refused());
        assert_eq!(
            decision.bypasses,
            [Bypass { name: "refs/heads/release/1.0".to_owned(), instead: format!("refused: {}", reasons::DELETION) }]
        );

        // A matching rule without bypass still applies to administrators: no bypass recorded.
        let rules =
            [rule("main", RuleSettings { allow_admin_bypass: true, ..RuleSettings::default() }), rule("ma*", RuleSettings::default())];
        let decision = decide_push(&rules, &admin, &[update("refs/heads/main", Kind::Delete)]);
        assert_eq!(decision.verdict.refusals[0].reason, reasons::DELETION);
        assert!(decision.bypasses.is_empty(), "the outcome did not change");

        // Restrictions intersect: a pusher must be allowed by every restricting rule.
        let restricted = RuleSettings { restrict_pushers: true, ..lenient };
        let only_listed = ProtectionRule { allowed_teams: vec![], ..rule("main", restricted) };
        let only_team = ProtectionRule { allowed_users: vec![], ..rule("m*", restricted) };
        for (actor, allowed) in [(Actor::ListedUser, false), (Actor::TeamMember, false)] {
            let decision =
                decide_push(&[only_listed.clone(), only_team.clone()], &actor.pusher(), &[update("refs/heads/main", Kind::Create)]);
            assert_eq!(!decision.verdict.is_refused(), allowed, "{actor:?}");
        }
        let both = Pusher::User { id: LISTED, access: RepoAccess::Write, teams: vec![TEAM] };
        assert!(!decide_push(&[only_listed, only_team], &both, &[update("refs/heads/main", Kind::Create)]).verdict.is_refused());
    }

    #[test]
    fn every_ref_of_a_push_is_decided() {
        let rules = [rule("main", RuleSettings::default())];
        let updates = [
            update("refs/heads/main", Kind::Delete),
            update("refs/heads/topic", Kind::Update),
            update("refs/pull/3/head", Kind::Create),
            update("refs/heads/main", Kind::Update),
        ];
        let decision = decide_push(&rules, &Actor::Writer.pusher(), &updates);
        let refused: Vec<_> = decision.verdict.refusals.iter().map(|r| (r.name.as_str(), r.reason.as_str())).collect();
        assert_eq!(refused, [("refs/heads/main", reasons::DELETION), ("refs/pull/3/head", reasons::PULL_REFS)]);
        assert!(decision.verdict.deny_non_fast_forwards, "the update of main is force-push protected");
        assert_eq!(decide_push(&rules, &Actor::Writer.pusher(), &[]).verdict, PushVerdict::allow());
    }

    #[test]
    fn merge_requirements() {
        let writer = Actor::Writer.pusher();
        assert_eq!(merge_rules(&[], &writer, "main"), MergeRules::unprotected());
        assert!(!MergeRules::unprotected().is_protected());

        let reviewed = RuleSettings {
            require_pull_request: true,
            required_approvals: 2,
            dismiss_stale_approvals: true,
            block_on_changes_requested: false,
            ..RuleSettings::default()
        };
        let rules = [
            rule("main", reviewed),
            rule("ma*", RuleSettings { required_approvals: 0, ..RuleSettings::default() }),
            rule("other", RuleSettings { restrict_pushers: true, ..RuleSettings::default() }),
        ];
        let merge = merge_rules(&rules, &writer, "main");
        assert_eq!(merge.patterns, ["main", "ma*"]);
        assert!(merge.is_protected() && merge.require_pull_request && merge.dismiss_stale_approvals);
        assert_eq!(merge.required_approvals, 2);
        assert!(!merge.block_on_changes_requested, "only rules requiring pull requests set review options");
        assert!(merge.merger_may_push && !merge.merger_may_bypass);

        let restricted = [rule("main", RuleSettings { restrict_pushers: true, allow_admin_bypass: true, ..reviewed })];
        let merge = merge_rules(&restricted, &writer, "main");
        assert!(
            !merge.merger_may_push && !merge.merger_may_bypass && merge.block_on_changes_requested == reviewed.block_on_changes_requested
        );
        assert!(merge_rules(&restricted, &Actor::ListedUser.pusher(), "main").merger_may_push);
        assert!(merge_rules(&restricted, &Actor::TeamMember.pusher(), "main").merger_may_push);
        let admin = merge_rules(&restricted, &Actor::Admin.pusher(), "main");
        assert!(!admin.merger_may_push && admin.merger_may_bypass);
        // Bypass needs every matching rule to allow it.
        let mixed = [restricted[0].clone(), rule("*", RuleSettings::default())];
        assert!(!merge_rules(&mixed, &Actor::Admin.pusher(), "main").merger_may_bypass);
        assert!(!merge_rules(&restricted, &Actor::DeployKey.pusher(), "main").merger_may_push);
        assert!(!merge.require_status_checks && merge.required_contexts.is_empty(), "contexts count only with the flag");
    }

    #[test]
    fn required_checks_are_the_union_of_the_matching_rules() {
        let checked = RuleSettings { require_status_checks: true, ..RuleSettings::default() };
        let rules = [
            ProtectionRule { required_contexts: vec![context("lint"), context("ci")], ..rule("main", checked) },
            ProtectionRule { required_contexts: vec![context("ci"), context("build")], ..rule("ma*", checked) },
            // Without the flag its contexts are not required.
            ProtectionRule { required_contexts: vec![context("docs")], ..rule("*", RuleSettings::default()) },
            ProtectionRule { required_contexts: vec![context("deploy")], ..rule("release/*", checked) },
        ];
        let merge = merge_rules(&rules, &Actor::Writer.pusher(), "main");
        assert!(merge.require_status_checks);
        assert_eq!(merge.required_contexts, [context("build"), context("ci"), context("lint")]);
        let other = merge_rules(&rules, &Actor::Writer.pusher(), "feature");
        assert!(other.is_protected() && !other.require_status_checks && other.required_contexts.is_empty());
        // Status checks never change what a push may do.
        let decision = decide_push(&rules, &Actor::Writer.pusher(), &[update("refs/heads/main", Kind::Update)]);
        assert!(!decision.verdict.is_refused());
    }

    #[test]
    fn deleting_and_listing_protected_branches() {
        let rules =
            [rule("main", RuleSettings::default()), rule("tmp/*", RuleSettings { deny_deletion: false, ..RuleSettings::default() })];
        let writer = Actor::Writer.pusher();
        assert!(!may_delete(&rules, &writer, "main"));
        assert!(may_delete(&rules, &writer, "tmp/x") && may_delete(&rules, &writer, "feature"));
        let bypassing = [rule("main", RuleSettings { allow_admin_bypass: true, ..RuleSettings::default() })];
        assert!(may_delete(&bypassing, &Actor::Admin.pusher(), "main") && !may_delete(&bypassing, &writer, "main"));
        assert!(is_protected(&rules, "tmp/x") && is_protected(&rules, "main") && !is_protected(&rules, "feature"));
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn filesystem_case_aliases_share_every_protection_check() {
        let settings = RuleSettings { require_pull_request: true, required_approvals: 2, ..RuleSettings::default() };
        let rules = [rule("main", settings), rule("résumé/*", settings)];
        let writer = Actor::Writer.pusher();
        for branch in ["MAIN", "Main", "RÉSUMÉ/Release"] {
            assert!(is_protected(&rules, branch), "{branch}");
            assert!(!may_delete(&rules, &writer, branch), "{branch}");
            let merge = merge_rules(&rules, &writer, branch);
            assert!(merge.require_pull_request && merge.required_approvals == 2, "{branch}");
            let name = format!("refs/heads/{branch}");
            let decision = decide_push(&rules, &writer, &[update(&name, Kind::Update)]);
            assert_eq!(decision.verdict.refusals[0].reason, reasons::PULL_REQUEST);
        }
        assert!(pattern("résumé").matches("RÉSUMÉ"), "exact patterns use Unicode case mappings too");
        assert!(pattern("RÉSUMÉ/*").matches("résumé/feature"), "case matching is symmetric");
        assert!(!pattern("main").matches("main/other"));
        assert!(!pattern("main").matches("maintain"));
        assert!(!decide_push(&[], &writer, &[update("refs/heads/MixedCase", Kind::Create)]).verdict.is_refused());
        let force = decide_push(&[rule("main", RuleSettings::default())], &writer, &[update("refs/heads/MAIN", Kind::Update)]);
        assert!(force.verdict.deny_non_fast_forwards);
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn filesystem_namespace_aliases_are_refused_before_git() {
        for actor in ACTORS {
            for kind in [Kind::Create, Kind::Update, Kind::Delete] {
                for name in ["refs/HEADS/main", "REFS/heads/main", "refs/Headſ/main"] {
                    let decision = decide_push(&[], &actor.pusher(), &[update(name, kind)]);
                    assert_eq!(decision.verdict.refusals[0].reason, reasons::NAMESPACE_CASE, "{name}");
                }
                for name in ["refs/PULL/1/head", "REFS/pull/1/head", "refs/Pull/1/merge"] {
                    let decision = decide_push(&[], &actor.pusher(), &[update(name, kind)]);
                    assert_eq!(decision.verdict.refusals[0].reason, reasons::PULL_REFS, "{name}");
                }
            }
        }
    }
}
