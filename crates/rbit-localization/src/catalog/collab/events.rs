//! Timeline event sentences: "alice added the bug label". Slot 0 is always the actor; the
//! other slots are listed on each variant (a label, a commit, a branch, an account…).
use super::super::fallback::{MessageFamily, MessageWriter};
use super::rich::{RichFamily, RichWriter};

/// How an issue or pull request was closed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseReason {
    Completed,
    NotPlanned,
    Merged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimelineEventMessage {
    Closed {
        reason: CloseReason,
    },
    /// Slot 1: the commit or pull request that closed it.
    ClosedIn {
        reason: CloseReason,
    },
    Reopened,
    /// Slot 1: the old title; 2: the new one.
    Renamed,
    Locked,
    Unlocked,
    /// Slot 1: the label.
    Labeled,
    Unlabeled,
    /// Slot 1: the milestone.
    Milestoned,
    Demilestoned,
    SelfAssigned,
    /// Slot 1: the assignee.
    Assigned,
    SelfUnassigned,
    Unassigned,
    /// Slot 1: the commit.
    CommitReferenced,
    /// Slot 1: the issue or pull request that mentions this one.
    MentionedInIssue,
    MentionedInPull,
    /// The mentioning thread is hidden from the viewer.
    Mentioned,
    /// Slot 2: the old head; 3: the new head.
    ForcePushed,
    /// Slot 1: the head branch; 2: the old head; 3: the new head.
    ForcePushedBranch,
    /// Slot 2: the new head.
    Pushed {
        count: u64,
    },
    /// Slot 1: the head branch; 2: the new head.
    PushedTo {
        count: u64,
    },
    /// Slot 1: the old base; 2: the new base.
    BaseChanged,
    /// Slot 1: the merge commit; 3: how it was merged.
    Merged,
    /// Slot 1: the merge commit; 2: the base branch; 3: how it was merged.
    MergedInto,
    ReviewRequestedSelf,
    /// Slot 1: the reviewer.
    ReviewRequested,
    ReviewRequestRemoved,
    /// Slot 1: the reviewer; 2: the review link ([`TimelineNoteMessage::Review`]).
    ReviewDismissedOf,
    /// Slot 2: the review link.
    ReviewDismissed,
    /// Slot 1: the conversation link ([`TimelineNoteMessage::ReviewConversation`]).
    ThreadResolved,
    ThreadReopened,
    /// Slot 1: the branch.
    HeadRefDeleted,
    ConvertedToDraft,
    ReadyForReview,
}

impl RichFamily for TimelineEventMessage {
    fn id(self) -> &'static str {
        match self {
            Self::Closed { .. } => "timeline.event.closed",
            Self::ClosedIn { .. } => "timeline.event.closed-in",
            Self::Reopened => "timeline.event.reopened",
            Self::Renamed => "timeline.event.renamed",
            Self::Locked => "timeline.event.locked",
            Self::Unlocked => "timeline.event.unlocked",
            Self::Labeled => "timeline.event.labeled",
            Self::Unlabeled => "timeline.event.unlabeled",
            Self::Milestoned => "timeline.event.milestoned",
            Self::Demilestoned => "timeline.event.demilestoned",
            Self::SelfAssigned => "timeline.event.self-assigned",
            Self::Assigned => "timeline.event.assigned",
            Self::SelfUnassigned => "timeline.event.self-unassigned",
            Self::Unassigned => "timeline.event.unassigned",
            Self::CommitReferenced => "timeline.event.commit-referenced",
            Self::MentionedInIssue => "timeline.event.mentioned-in-issue",
            Self::MentionedInPull => "timeline.event.mentioned-in-pull",
            Self::Mentioned => "timeline.event.mentioned",
            Self::ForcePushed => "timeline.event.force-pushed",
            Self::ForcePushedBranch => "timeline.event.force-pushed-branch",
            Self::Pushed { .. } => "timeline.event.pushed",
            Self::PushedTo { .. } => "timeline.event.pushed-to",
            Self::BaseChanged => "timeline.event.base-changed",
            Self::Merged => "timeline.event.merged",
            Self::MergedInto => "timeline.event.merged-into",
            Self::ReviewRequestedSelf => "timeline.event.review-requested-self",
            Self::ReviewRequested => "timeline.event.review-requested",
            Self::ReviewRequestRemoved => "timeline.event.review-request-removed",
            Self::ReviewDismissedOf => "timeline.event.review-dismissed-of",
            Self::ReviewDismissed => "timeline.event.review-dismissed",
            Self::ThreadResolved => "timeline.event.thread-resolved",
            Self::ThreadReopened => "timeline.event.thread-reopened",
            Self::HeadRefDeleted => "timeline.event.head-ref-deleted",
            Self::ConvertedToDraft => "timeline.event.converted-to-draft",
            Self::ReadyForReview => "timeline.event.ready-for-review",
        }
    }

    fn english(self, text: &mut RichWriter) {
        text.slot(0);
        match self {
            Self::Closed { reason } | Self::ClosedIn { reason } => {
                text.literal(match reason {
                    CloseReason::Completed => " closed this as completed",
                    CloseReason::NotPlanned => " closed this as not planned",
                    CloseReason::Merged => " closed this as merged",
                });
                if matches!(self, Self::ClosedIn { .. }) {
                    then(text, " in ", 1);
                }
            }
            Self::Renamed => {
                then(text, " changed the title ", 1);
                then(text, " ", 2);
            }
            Self::Labeled | Self::Unlabeled => {
                then(text, if self == Self::Labeled { " added the " } else { " removed the " }, 1);
                text.literal(" label");
            }
            Self::Milestoned | Self::Demilestoned => {
                then(text, if self == Self::Milestoned { " added this to the " } else { " removed this from the " }, 1);
                text.literal(" milestone");
            }
            Self::Assigned => then(text, " assigned ", 1),
            Self::Unassigned => then(text, " unassigned ", 1),
            Self::CommitReferenced => then(text, " referenced this in commit ", 1),
            Self::MentionedInIssue => then(text, " mentioned this in issue ", 1),
            Self::MentionedInPull => then(text, " mentioned this in pull request ", 1),
            Self::ForcePushed => {
                then(text, " force-pushed from ", 2);
                then(text, " to ", 3);
            }
            Self::ForcePushedBranch => {
                then(text, " force-pushed the ", 1);
                then(text, " branch from ", 2);
                then(text, " to ", 3);
            }
            Self::Pushed { count } | Self::PushedTo { count } => {
                text.literal(" added ");
                text.number(count);
                text.literal(if count == 1 { " commit" } else { " commits" });
                if matches!(self, Self::PushedTo { .. }) {
                    then(text, " to ", 1);
                }
                then(text, ", now at ", 2);
            }
            Self::BaseChanged => {
                then(text, " changed the base branch from ", 1);
                then(text, " to ", 2);
            }
            Self::Merged | Self::MergedInto => {
                then(text, " merged commit ", 1);
                if self == Self::MergedInto {
                    then(text, " into ", 2);
                }
                then(text, " ", 3);
            }
            Self::ReviewRequested => then(text, " requested a review from ", 1),
            Self::ReviewRequestRemoved => then(text, " removed the request for review from ", 1),
            Self::ReviewDismissedOf => {
                then(text, " dismissed ", 1);
                then(text, "’s ", 2);
            }
            Self::ReviewDismissed => then(text, " dismissed a ", 2),
            Self::ThreadResolved => then(text, " resolved ", 1),
            Self::ThreadReopened => then(text, " reopened ", 1),
            Self::HeadRefDeleted => {
                then(text, " deleted the ", 1);
                text.literal(" branch");
            }
            sentence => text.literal(match sentence {
                Self::Reopened => " reopened this",
                Self::Locked => " locked the conversation and limited it to collaborators",
                Self::Unlocked => " unlocked this conversation",
                Self::SelfAssigned => " self-assigned this",
                Self::SelfUnassigned => " removed their assignment",
                Self::Mentioned => " mentioned this",
                Self::ReviewRequestedSelf => " requested a review from themselves",
                Self::ConvertedToDraft => " marked this pull request as draft",
                _ => " marked this pull request as ready for review",
            }),
        }
    }
}

/// `words`, then slot `slot`.
fn then(text: &mut RichWriter, words: &str, slot: usize) {
    text.literal(words);
    text.slot(slot);
}

/// Words shown inside an event's links and notes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimelineNoteMessage {
    Review,
    ReviewConversation,
    MergeCommit,
    Squash,
    Rebase,
    OutsideInstance,
}

impl MessageFamily for TimelineNoteMessage {
    fn id(self) -> &'static str {
        match self {
            Self::Review => "timeline.note.review",
            Self::ReviewConversation => "timeline.note.review-conversation",
            Self::MergeCommit => "timeline.note.merge-commit",
            Self::Squash => "timeline.note.squash",
            Self::Rebase => "timeline.note.rebase",
            Self::OutsideInstance => "timeline.note.outside-instance",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        text.literal(match self {
            Self::Review => "review",
            Self::ReviewConversation => "a review conversation",
            Self::MergeCommit => "(merge commit)",
            Self::Squash => "(squash)",
            Self::Rebase => "(rebase)",
            Self::OutsideInstance => "(outside this instance)",
        });
    }
}
