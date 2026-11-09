//! Approval tallies and the merge box: every requirement of merging a pull request,
//! decided from its facts and the protection rules of the base branch.

use std::collections::HashMap;

use rbit_contracts::AccountId;

use super::{ChecksStatus, Mergeability, Review, ReviewState};
use crate::statuses::{RequiredChecks, StatusContext};

/// What merging into a branch requires of the merger, over status contexts.
pub type MergeRules = rbit_access::protection::MergeRules<StatusContext>;

/// Where the reviews of one pull request stand, for the merge requirements.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewTally {
    /// Writers whose latest decisive review approves (and counts: see [`tally`]).
    pub approvals: Vec<AccountId>,
    /// Writers whose latest decisive review requests changes.
    pub changes_requested: Vec<AccountId>,
}

/// Counts approvals and change requests.
///
/// For every reviewer, only their **latest decisive** review counts: the newest submitted
/// review that is `approved` or `changes_requested` (pending, comment-only and dismissed
/// reviews neither approve nor withdraw an approval, as on GitHub). Only reviewers for
/// whom `is_writer` holds count, never the pull request's `author`. With
/// `dismiss_stale_approvals`, an approval counts only if it was made on `head_sha`;
/// change requests count regardless of the commit.
pub fn tally(
    reviews: &[Review],
    author: Option<AccountId>,
    head_sha: &str,
    dismiss_stale_approvals: bool,
    is_writer: impl Fn(AccountId) -> bool,
) -> ReviewTally {
    let mut latest: HashMap<AccountId, &Review> = HashMap::new();
    for review in reviews {
        let Some(reviewer) = review.reviewer else { continue };
        if !matches!(review.state, ReviewState::Approved | ReviewState::ChangesRequested) || Some(reviewer) == author {
            continue;
        }
        let key = |r: &Review| (r.submitted_at.unwrap_or(r.created_at), r.id);
        match latest.get(&reviewer) {
            Some(known) if key(known) >= key(review) => {}
            _ => {
                latest.insert(reviewer, review);
            }
        }
    }
    let mut tally = ReviewTally::default();
    for (reviewer, review) in latest {
        if !is_writer(reviewer) {
            continue;
        }
        match review.state {
            ReviewState::Approved if !dismiss_stale_approvals || review.commit_sha == head_sha => tally.approvals.push(reviewer),
            ReviewState::ChangesRequested => tally.changes_requested.push(reviewer),
            _ => {}
        }
    }
    tally.approvals.sort();
    tally.changes_requested.sort();
    tally
}

// ------------------------------------------------------------------ requirements

/// One line of the merge box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    Open,
    NotDraft,
    HeadBranchExists,
    /// The trial merge is clean.
    NoConflicts(Mergeability),
    /// Approvals by writers, counted as in [`tally`].
    Approvals {
        have: usize,
        need: u8,
    },
    /// No writer's latest review requests changes.
    NoChangesRequested {
        count: usize,
    },
    /// The merger may push to the base branch (restricted pushers).
    MergerMayPush,
    /// The head commit's statuses as a whole: informational, never blocks merging.
    Checks(ChecksStatus),
    /// Every context the protection rules require passes on the head commit.
    RequiredChecks(RequiredChecks),
}

impl Requirement {
    /// Requirements a protection bypass may skip: the protection rules themselves.
    pub const fn is_protection(self) -> bool {
        matches!(self, Self::Approvals { .. } | Self::NoChangesRequested { .. } | Self::MergerMayPush | Self::RequiredChecks(_))
    }

    /// The sentence shown in the merge box.
    pub fn describe(self, met: bool) -> String {
        match (self, met) {
            (Self::Open, _) => if met { "The pull request is open" } else { "The pull request is closed" }.to_owned(),
            (Self::NotDraft, true) => "Ready for review".to_owned(),
            (Self::NotDraft, false) => "This pull request is still a draft".to_owned(),
            (Self::HeadBranchExists, true) => "The head branch exists".to_owned(),
            (Self::HeadBranchExists, false) => "The head branch was deleted".to_owned(),
            (Self::NoConflicts(state), _) => match state {
                Mergeability::Clean => "No conflicts with the base branch",
                Mergeability::Conflict => "This branch has conflicts that must be resolved",
                Mergeability::Unknown => "Checking for conflicts…",
                Mergeability::Error => "Mergeability could not be checked",
            }
            .to_owned(),
            (Self::Approvals { have, need }, _) => {
                let noun = if need == 1 { "approval" } else { "approvals" };
                format!("{have} of {need} required {noun}")
            }
            (Self::NoChangesRequested { .. }, true) => "No changes requested".to_owned(),
            (Self::NoChangesRequested { count }, false) => {
                format!("{count} {} requested changes", if count == 1 { "reviewer" } else { "reviewers" })
            }
            (Self::MergerMayPush, true) => "You may push to the base branch".to_owned(),
            (Self::MergerMayPush, false) => "You are not allowed to push to the base branch".to_owned(),
            (Self::Checks(status), _) => match status {
                ChecksStatus::NotConfigured => "No status checks",
                ChecksStatus::Pending => "Some checks have not completed yet",
                ChecksStatus::Passing => "All checks have passed",
                ChecksStatus::Failing => "Some checks were not successful",
            }
            .to_owned(),
            (Self::RequiredChecks(checks), _) => {
                let plural = |count: usize, one: &str, many: &str| if count == 1 { one.to_owned() } else { many.to_owned() };
                if checks.all_pass() {
                    format!("{} {} passed", checks.required, plural(checks.required, "required check", "required checks"))
                } else if checks.failing > 0 {
                    format!("{} of {} required checks failed", checks.failing, checks.required)
                } else {
                    format!("Waiting for {} of {} required checks", checks.waiting(), checks.required)
                }
            }
        }
    }
}

/// A requirement and whether it is met.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequirementCheck {
    pub requirement: Requirement,
    pub met: bool,
}

/// What [`evaluate`] needs to know about the pull request.
#[derive(Debug, Clone, Copy)]
pub struct MergeFacts<'a> {
    pub is_open: bool,
    pub is_draft: bool,
    pub head_branch_exists: bool,
    pub mergeable: Mergeability,
    pub tally: &'a ReviewTally,
    pub checks: ChecksStatus,
    /// How the contexts in [`MergeRules::required_contexts`] stand on the head commit.
    pub required_checks: RequiredChecks,
}

/// The merge box: every requirement with its state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeEvaluation {
    pub checks: Vec<RequirementCheck>,
    /// The merger may merge despite unmet protection requirements (after confirming).
    pub may_bypass: bool,
}

impl MergeEvaluation {
    /// Every requirement is met.
    pub fn can_merge(&self) -> bool {
        self.checks.iter().all(|check| check.met)
    }

    /// Every requirement but protection ones is met, and the merger may bypass those.
    pub fn can_merge_with_bypass(&self) -> bool {
        self.may_bypass && self.checks.iter().all(|check| check.met || check.requirement.is_protection())
    }

    /// Some protection requirement is unmet (GitHub's `blocked`).
    pub fn is_blocked(&self) -> bool {
        self.checks.iter().any(|check| !check.met && check.requirement.is_protection())
    }
}

/// Evaluates the merge requirements of `facts` against `rules`.
pub fn evaluate(facts: &MergeFacts<'_>, rules: &MergeRules) -> MergeEvaluation {
    let check = |requirement, met| RequirementCheck { requirement, met };
    let mut checks = vec![
        check(Requirement::Open, facts.is_open),
        check(Requirement::NotDraft, !facts.is_draft),
        check(Requirement::HeadBranchExists, facts.head_branch_exists),
        check(Requirement::NoConflicts(facts.mergeable), facts.mergeable == Mergeability::Clean),
    ];
    if rules.required_approvals > 0 {
        let have = facts.tally.approvals.len();
        checks.push(check(Requirement::Approvals { have, need: rules.required_approvals }, have >= usize::from(rules.required_approvals)));
    }
    if rules.block_on_changes_requested {
        let count = facts.tally.changes_requested.len();
        checks.push(check(Requirement::NoChangesRequested { count }, count == 0));
    }
    if !rules.merger_may_push {
        checks.push(check(Requirement::MergerMayPush, false));
    }
    if rules.require_status_checks {
        checks.push(check(Requirement::RequiredChecks(facts.required_checks), facts.required_checks.all_pass()));
    }
    // Statuses nobody requires are shown but never block, as on GitHub.
    if facts.checks != ChecksStatus::NotConfigured {
        checks.push(check(Requirement::Checks(facts.checks), true));
    }
    MergeEvaluation { checks, may_bypass: rules.merger_may_bypass }
}
