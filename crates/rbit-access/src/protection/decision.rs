//! The pure push and merge decisions made from the rules matching a branch.

use super::{ProtectionRule, namespace_refusal};
use crate::RepoAccess;

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

/// A ref update a push asks for, as the decisions read it.
pub trait RefChange {
    /// The full ref name, e.g. `refs/heads/main`.
    fn name(&self) -> &str;
    /// The ref does not exist yet.
    fn is_created(&self) -> bool;
    /// The update deletes the ref.
    fn is_deleted(&self) -> bool;
}

/// A branch update the server itself asks about (deleting a head branch, committing in
/// the web editor), shaped like the matching push command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchChange {
    name: String,
    deleted: bool,
}

impl BranchChange {
    /// Updating the existing `branch`.
    pub fn update(branch: &str) -> Self {
        Self { name: format!("refs/heads/{branch}"), deleted: false }
    }

    /// Deleting the existing `branch`.
    pub fn deletion(branch: &str) -> Self {
        Self { name: format!("refs/heads/{branch}"), deleted: true }
    }
}

impl RefChange for BranchChange {
    fn name(&self) -> &str {
        &self.name
    }
    fn is_created(&self) -> bool {
        false
    }
    fn is_deleted(&self) -> bool {
        self.deleted
    }
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
    pub fn refuse_all<R: RefChange>(updates: &[R], reason: &str) -> Self {
        let refusals = updates.iter().map(|update| Refusal { name: update.name().to_owned(), reason: reason.to_owned() }).collect();
        Self { refusals, deny_non_fast_forwards: false }
    }

    pub fn is_refused(&self) -> bool {
        !self.refusals.is_empty()
    }
}

/// What merging a pull request into a branch requires of the merger.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "independent rule flags")]
pub struct MergeRules<C> {
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
    pub required_contexts: Vec<C>,
}

impl<C> MergeRules<C> {
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

/// The pusher as the decisions see them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pusher<U, T> {
    DeployKey {
        access: RepoAccess,
    },
    /// A user, with the teams they are on (at least those the rules name).
    User {
        id: U,
        access: RepoAccess,
        teams: Vec<T>,
    },
}

impl<U: PartialEq, T: PartialEq> Pusher<U, T> {
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
    fn allowed_by<C>(&self, rule: &ProtectionRule<U, T, C>) -> bool {
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
fn apply<U: PartialEq, T: PartialEq, C>(
    rule: &ProtectionRule<U, T, C>,
    pusher: &Pusher<U, T>,
    update: &impl RefChange,
    outcome: &mut Outcome,
) {
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
pub fn decide_push<U: PartialEq, T: PartialEq, C>(
    rules: &[ProtectionRule<U, T, C>],
    pusher: &Pusher<U, T>,
    updates: &[impl RefChange],
) -> PushDecision {
    let mut decision = PushDecision::default();
    for update in updates {
        if let Some(reason) = namespace_refusal(update.name()) {
            decision.verdict.refusals.push(Refusal { name: update.name().to_owned(), reason: reason.to_owned() });
            continue;
        }
        if !pusher.access().can_write() {
            decision.verdict.refusals.push(Refusal { name: update.name().to_owned(), reason: reasons::NO_WRITE.to_owned() });
            continue;
        }
        let Some(branch) = update.name().strip_prefix("refs/heads/") else { continue };
        let (mut enforced, mut strict) = (Outcome::default(), Outcome::default());
        for rule in rules.iter().filter(|rule| rule.matches(branch)) {
            apply(rule, pusher, update, &mut strict);
            if !(rule.settings.allow_admin_bypass && pusher.is_admin()) {
                apply(rule, pusher, update, &mut enforced);
            }
        }
        if enforced != strict {
            decision.bypasses.push(Bypass { name: update.name().to_owned(), instead: strict.description() });
        }
        if let Some(reason) = enforced.refusal {
            decision.verdict.refusals.push(Refusal { name: update.name().to_owned(), reason: reason.to_owned() });
        } else if enforced.deny_non_fast_forwards {
            decision.verdict.deny_non_fast_forwards = true;
        }
    }
    decision
}

/// What merging into `branch` requires of `merger`, from the rules matching it. Approval
/// options count only on rules that require a pull request.
pub fn merge_rules<U: PartialEq, T: PartialEq, C: Ord + Clone>(
    rules: &[ProtectionRule<U, T, C>],
    merger: &Pusher<U, T>,
    branch: &str,
) -> MergeRules<C> {
    let matching: Vec<&ProtectionRule<U, T, C>> = rules.iter().filter(|rule| rule.matches(branch)).collect();
    if matching.is_empty() {
        return MergeRules::unprotected();
    }
    let with_pull_requests = || matching.iter().filter(|rule| rule.settings.require_pull_request);
    let with_checks = || matching.iter().filter(|rule| rule.settings.require_status_checks);
    let mut required_contexts: Vec<C> = with_checks().flat_map(|rule| rule.required_contexts.iter().cloned()).collect();
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
pub fn may_delete<U: PartialEq, T: PartialEq, C>(rules: &[ProtectionRule<U, T, C>], pusher: &Pusher<U, T>, branch: &str) -> bool {
    let update = BranchChange::deletion(branch);
    !decide_push(rules, pusher, &[update]).verdict.is_refused()
}

/// Whether any rule matches `branch`.
pub fn is_protected<U, T, C>(rules: &[ProtectionRule<U, T, C>], branch: &str) -> bool {
    rules.iter().any(|rule| rule.matches(branch))
}
