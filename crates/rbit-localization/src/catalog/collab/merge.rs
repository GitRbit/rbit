//! The merge box: each requirement and the merge form.
use super::super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MergeMessage<'a> {
    /// The accessible name of the merge box.
    Merge,
    ConflictingFiles,
    WorkInProgress,
    ReadyForReview,
    MergeMethod,
    CreateMergeCommit,
    SquashAndMerge,
    RebaseAndMerge,
    EditMessage,
    Message,
    /// The title placeholder: the default title of a merge commit and of a squash.
    TitlePlaceholder {
        title: &'a str,
        squash: &'a str,
    },
    DeleteHead,
    Bypass,
    MergePull,
    WriteAccessOnly,
    /// Requirements, met or not.
    PullOpen,
    PullClosed,
    StillDraft,
    HeadExists,
    HeadDeleted,
    NoConflicts,
    Conflicts,
    CheckingConflicts,
    MergeabilityError,
    Approvals {
        have: u64,
        need: u64,
    },
    NoChangesRequested,
    ChangesRequestedBy {
        count: u64,
    },
    MayPush,
    MayNotPush,
    NoChecks,
    ChecksPending,
    ChecksPassed,
    ChecksFailed,
    RequiredPassed {
        count: u64,
    },
    RequiredFailed {
        failing: u64,
        required: u64,
    },
    RequiredWaiting {
        waiting: u64,
        required: u64,
    },
}

impl MessageFamily for MergeMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::Merge => "merge.label",
            Self::ConflictingFiles => "merge.conflicting-files",
            Self::WorkInProgress => "merge.draft.text",
            Self::ReadyForReview => "merge.draft.ready",
            Self::MergeMethod => "merge.form.method",
            Self::CreateMergeCommit => "merge.form.strategy-merge",
            Self::SquashAndMerge => "merge.form.strategy-squash",
            Self::RebaseAndMerge => "merge.form.strategy-rebase",
            Self::EditMessage => "merge.form.edit-message",
            Self::Message => "merge.form.message",
            Self::TitlePlaceholder { .. } => "merge.form.title-placeholder",
            Self::DeleteHead => "merge.form.delete-head",
            Self::Bypass => "merge.form.bypass",
            Self::MergePull => "merge.form.submit",
            Self::WriteAccessOnly => "merge.write-access-only",
            Self::PullOpen => "merge.requirement.open",
            Self::PullClosed => "merge.requirement.closed",
            Self::StillDraft => "merge.requirement.draft",
            Self::HeadExists => "merge.requirement.head-exists",
            Self::HeadDeleted => "merge.requirement.head-deleted",
            Self::NoConflicts => "merge.requirement.no-conflicts",
            Self::Conflicts => "merge.requirement.conflicts",
            Self::CheckingConflicts => "merge.requirement.checking",
            Self::MergeabilityError => "merge.requirement.error",
            Self::Approvals { .. } => "merge.requirement.approvals",
            Self::NoChangesRequested => "merge.requirement.no-changes-requested",
            Self::ChangesRequestedBy { .. } => "merge.requirement.changes-requested",
            Self::MayPush => "merge.requirement.may-push",
            Self::MayNotPush => "merge.requirement.may-not-push",
            Self::NoChecks => "merge.requirement.no-checks",
            Self::ChecksPending => "merge.requirement.checks-pending",
            Self::ChecksPassed => "merge.requirement.checks-passed",
            Self::ChecksFailed => "merge.requirement.checks-failed",
            Self::RequiredPassed { .. } => "merge.requirement.required-passed",
            Self::RequiredFailed { .. } => "merge.requirement.required-failed",
            Self::RequiredWaiting { .. } => "merge.requirement.required-waiting",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::TitlePlaceholder { title, squash } => {
                text.argument(title);
                text.literal(" — or, when squashing, ");
                text.argument(squash);
            }
            Self::Approvals { have, need } => {
                text.number(have);
                text.literal(" of ");
                text.number(need);
                text.literal(if need == 1 { " required approval" } else { " required approvals" });
            }
            Self::ChangesRequestedBy { count } => {
                text.number(count);
                text.literal(if count == 1 { " reviewer requested changes" } else { " reviewers requested changes" });
            }
            Self::RequiredPassed { count } => {
                text.number(count);
                text.literal(if count == 1 { " required check passed" } else { " required checks passed" });
            }
            Self::RequiredFailed { failing, required } => {
                text.number(failing);
                text.literal(" of ");
                text.number(required);
                text.literal(" required checks failed");
            }
            Self::RequiredWaiting { waiting, required } => {
                text.literal("Waiting for ");
                text.number(waiting);
                text.literal(" of ");
                text.number(required);
                text.literal(" required checks");
            }
            literal => text.literal(literal_text(literal)),
        }
    }
}

/// The English text of the messages without arguments.
const fn literal_text(message: MergeMessage<'_>) -> &'static str {
    match message {
        MergeMessage::Merge => "Merge",
        MergeMessage::ConflictingFiles => "Conflicting files",
        MergeMessage::WorkInProgress => "This pull request is still a work in progress.",
        MergeMessage::ReadyForReview => "Ready for review",
        MergeMessage::MergeMethod => "Merge method",
        MergeMessage::CreateMergeCommit => "Create a merge commit",
        MergeMessage::SquashAndMerge => "Squash and merge",
        MergeMessage::RebaseAndMerge => "Rebase and merge",
        MergeMessage::EditMessage => "Edit the commit message",
        MergeMessage::Message => "Message",
        MergeMessage::DeleteHead => "Delete the head branch after merging",
        MergeMessage::Bypass => "Merge without meeting the branch protection requirements (this is audited)",
        MergeMessage::MergePull => "Merge pull request",
        MergeMessage::WriteAccessOnly => "Only people with write access to this repository can merge.",
        MergeMessage::PullOpen => "The pull request is open",
        MergeMessage::PullClosed => "The pull request is closed",
        MergeMessage::StillDraft => "This pull request is still a draft",
        MergeMessage::HeadExists => "The head branch exists",
        MergeMessage::HeadDeleted => "The head branch was deleted",
        MergeMessage::NoConflicts => "No conflicts with the base branch",
        MergeMessage::Conflicts => "This branch has conflicts that must be resolved",
        MergeMessage::CheckingConflicts => "Checking for conflicts…",
        MergeMessage::MergeabilityError => "Mergeability could not be checked",
        MergeMessage::NoChangesRequested => "No changes requested",
        MergeMessage::MayPush => "You may push to the base branch",
        MergeMessage::MayNotPush => "You are not allowed to push to the base branch",
        MergeMessage::NoChecks => "No status checks",
        MergeMessage::ChecksPending => "Some checks have not completed yet",
        MergeMessage::ChecksPassed => "All checks have passed",
        MergeMessage::ChecksFailed => "Some checks were not successful",
        MergeMessage::TitlePlaceholder { .. }
        | MergeMessage::Approvals { .. }
        | MergeMessage::ChangesRequestedBy { .. }
        | MergeMessage::RequiredPassed { .. }
        | MergeMessage::RequiredFailed { .. }
        | MergeMessage::RequiredWaiting { .. } => "",
    }
}
