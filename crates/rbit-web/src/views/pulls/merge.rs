//! Pull request merge and closed-state view models.

use rbit_core::pull::{ChecksStatus, Requirement};
use rbit_localization::MergeMessage;

use super::{MergeBox, MergeStrategy, Mergeability, PullContext, PullUrls, User};

/// One requirement line.
#[derive(Debug, Clone)]
pub struct RequirementView {
    pub text: MergeMessage<'static>,
    pub met: bool,
    /// `text-success`, `text-danger` or `text-warning` (checking).
    pub tone: &'static str,
    pub icon: &'static str,
}

/// The merge box at the end of the conversation.
#[derive(Debug, Clone)]
#[expect(clippy::struct_excessive_bools, reason = "independent display flags")]
pub struct MergeBoxView {
    pub anchor: &'static str,
    pub requirements: Vec<RequirementView>,
    pub conflict_paths: Vec<String>,
    /// The head commit's checks, required ones first (milestone 3, S).
    pub checks: Option<crate::views::statuses::ChecksView>,
    pub checking: bool,
    pub can_merge: bool,
    pub ready: bool,
    pub needs_bypass: bool,
    pub can_delete_head: bool,
    pub action_url: String,
    pub expected_head: String,
    pub expected_base: String,
    pub default_title: String,
    pub default_message: String,
    pub default_squash_title: String,
    pub default_squash_message: String,
    pub strategies: Vec<(&'static str, MergeMessage<'static>)>,
    pub draft_url: String,
    pub can_mark_ready: bool,
}

impl MergeBoxView {
    pub fn new(merge_box: &MergeBox, ctx: &PullContext, urls: &PullUrls, viewer: Option<&User>) -> Self {
        let requirements = merge_box
            .evaluation
            .checks
            .iter()
            .map(|check| {
                let checking = match check.requirement {
                    Requirement::NoConflicts(Mergeability::Unknown) => true,
                    // Milestone 3 (S): required checks still running are pending, not failed.
                    Requirement::RequiredChecks(checks) => checks.failing == 0,
                    _ => false,
                };
                let (tone, icon) = match (check.requirement, check.met, checking) {
                    // Statuses nobody requires: always "met", shown by their state.
                    (Requirement::Checks(ChecksStatus::Failing), ..) => ("text-danger", "x"),
                    (Requirement::Checks(ChecksStatus::NotConfigured), ..) => ("text-ink-faint", "circle-dot"),
                    (Requirement::Checks(ChecksStatus::Pending), ..) | (Requirement::RequiredChecks(_), false, true) => {
                        ("text-warning", "circle-dot")
                    }
                    (_, true, _) => ("text-success", "check"),
                    (_, false, true) => ("text-warning", "refresh"),
                    (_, false, false) => ("text-danger", "x"),
                };
                let met = check.met && !matches!(check.requirement, Requirement::Checks(ChecksStatus::Failing | ChecksStatus::Pending));
                RequirementView { text: requirement_message(check.requirement, check.met), met, tone, icon }
            })
            .collect();
        let ready = merge_box.evaluation.can_merge();
        // Milestone 3 (S): when only running required checks are missing, the box is
        // "pending", not failed.
        let waiting_for_checks = !ready
            && merge_box
                .evaluation
                .checks
                .iter()
                .filter(|check| !check.met)
                .all(|check| matches!(check.requirement, rbit_core::pull::Requirement::RequiredChecks(checks) if checks.failing == 0));
        let is_author = viewer.is_some_and(|viewer| ctx.issue().is_author(viewer));
        Self {
            anchor: "merge-box",
            requirements,
            conflict_paths: merge_box.conflict_paths.clone(),
            checks: crate::views::statuses::ChecksView::new(&merge_box.statuses, &merge_box.rules.required_contexts),
            checking: merge_box.mergeable == Mergeability::Unknown || waiting_for_checks,
            can_merge: merge_box.can_merge,
            ready,
            needs_bypass: !ready && merge_box.evaluation.can_merge_with_bypass(),
            can_delete_head: merge_box.can_delete_head,
            action_url: urls.merge.clone(),
            expected_head: ctx.pull.head_sha.clone(),
            expected_base: ctx.pull.base_branch.clone(),
            default_title: merge_box.default_title.clone(),
            default_message: merge_box.default_message.clone(),
            default_squash_title: merge_box.default_squash_title.clone(),
            default_squash_message: merge_box.default_squash_message.clone(),
            strategies: [MergeStrategy::Merge, MergeStrategy::Squash, MergeStrategy::Rebase]
                .into_iter()
                .map(|strategy| {
                    let label = match strategy {
                        MergeStrategy::Merge => MergeMessage::CreateMergeCommit,
                        MergeStrategy::Squash => MergeMessage::SquashAndMerge,
                        MergeStrategy::Rebase => MergeMessage::RebaseAndMerge,
                    };
                    (strategy.as_str(), label)
                })
                .collect(),
            draft_url: urls.draft.clone(),
            can_mark_ready: ctx.pull.is_draft && !ctx.repo().repo.repo.is_archived() && (is_author || ctx.repo().access.can_write()),
        }
    }
}

/// The merge box sentence of `requirement`, as `Requirement::describe` words it.
fn requirement_message(requirement: Requirement, met: bool) -> MergeMessage<'static> {
    let count = |count: usize| count as u64;
    match (requirement, met) {
        (Requirement::Open, true) => MergeMessage::PullOpen,
        (Requirement::Open, false) => MergeMessage::PullClosed,
        (Requirement::NotDraft, true) => MergeMessage::ReadyForReview,
        (Requirement::NotDraft, false) => MergeMessage::StillDraft,
        (Requirement::HeadBranchExists, true) => MergeMessage::HeadExists,
        (Requirement::HeadBranchExists, false) => MergeMessage::HeadDeleted,
        (Requirement::NoConflicts(state), _) => match state {
            Mergeability::Clean => MergeMessage::NoConflicts,
            Mergeability::Conflict => MergeMessage::Conflicts,
            Mergeability::Unknown => MergeMessage::CheckingConflicts,
            Mergeability::Error => MergeMessage::MergeabilityError,
        },
        (Requirement::Approvals { have, need }, _) => MergeMessage::Approvals { have: count(have), need: need.into() },
        (Requirement::NoChangesRequested { .. }, true) => MergeMessage::NoChangesRequested,
        (Requirement::NoChangesRequested { count: reviewers }, false) => MergeMessage::ChangesRequestedBy { count: count(reviewers) },
        (Requirement::MergerMayPush, true) => MergeMessage::MayPush,
        (Requirement::MergerMayPush, false) => MergeMessage::MayNotPush,
        (Requirement::Checks(status), _) => match status {
            ChecksStatus::NotConfigured => MergeMessage::NoChecks,
            ChecksStatus::Pending => MergeMessage::ChecksPending,
            ChecksStatus::Passing => MergeMessage::ChecksPassed,
            ChecksStatus::Failing => MergeMessage::ChecksFailed,
        },
        (Requirement::RequiredChecks(checks), _) if checks.all_pass() => MergeMessage::RequiredPassed { count: count(checks.required) },
        (Requirement::RequiredChecks(checks), _) if checks.failing > 0 => {
            MergeMessage::RequiredFailed { failing: count(checks.failing), required: count(checks.required) }
        }
        (Requirement::RequiredChecks(checks), _) => {
            MergeMessage::RequiredWaiting { waiting: count(checks.waiting()), required: count(checks.required) }
        }
    }
}

/// After merging or closing: the head branch can be deleted.
#[derive(Debug, Clone)]
pub struct ClosedBoxView {
    pub merged: bool,
    pub merge_commit: Option<(String, String)>,
    pub can_delete_branch: bool,
    pub delete_url: String,
    pub head_label: String,
}
