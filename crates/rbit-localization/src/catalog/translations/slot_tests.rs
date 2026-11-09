//! Rich sentences keep their slots in every locale: a translation writes exactly the slots
//! its English arm writes, each as often, in any order. Templates render a slot's markup
//! (a link, a form, a time) once per slot, so a dropped slot loses that markup silently and
//! a duplicated one repeats it. Adding a variant fails to compile here until it is listed.
use std::fmt::Debug;

use crate::catalog::collab::rich::{RichFamily, RichPart, RichWriter, render};
use crate::catalog::collab::{CloseReason, CollabSentence, TimelineEventMessage};
use crate::locale::Locale;

const LOCALES: [Locale; 11] = [
    Locale::English,
    Locale::Spanish,
    Locale::ChineseSimplified,
    Locale::PortugueseBrazil,
    Locale::Japanese,
    Locale::German,
    Locale::Russian,
    Locale::French,
    Locale::Korean,
    Locale::PseudoExpanded,
    Locale::PseudoRtl,
];

/// Counts that reach every CLDR category of every locale (one, few, many, other).
const COUNTS: [u64; 9] = [0, 1, 2, 3, 5, 11, 21, 22, 1_000_000];

fn slots<M: RichFamily>(locale: Locale, message: M) -> Vec<usize> {
    let mut slots: Vec<usize> = render(locale, message).parts().iter().filter_map(RichPart::slot).collect();
    slots.sort_unstable();
    slots
}

/// Every message of `messages` whose slots in some locale differ from its English slots.
fn slot_mismatches<M: RichFamily + Debug>(messages: &[M]) -> Vec<String> {
    let mut mismatches = Vec::new();
    for &message in messages {
        let english = slots(Locale::English, message);
        for locale in LOCALES {
            let translated = slots(locale, message);
            if translated != english {
                mismatches.push(format!("{} ({message:?}) in {}: slots {translated:?}, English {english:?}", message.id(), locale.tag()));
            }
        }
    }
    mismatches
}

fn sentences() -> Vec<CollabSentence<'static>> {
    fn listed(message: CollabSentence<'_>) {
        use CollabSentence as S;
        match message {
            S::OpenCount { .. }
            | S::ClosedCount { .. }
            | S::IssueSearchHelp
            | S::PullSearchHelp
            | S::OpenedIssue { .. }
            | S::OpenedPull { .. }
            | S::RowOpened { .. }
            | S::RowClosed { .. }
            | S::RowClosedBy { .. }
            | S::RowMergedBy { .. }
            | S::NoOneAssignYourself
            | S::PercentComplete
            | S::WantsToMerge
            | S::MergedInto
            | S::ProposedToMerge
            | S::SignInToOpenPull
            | S::SignInToCommentOnLines
            | S::SignInToJoin
            | S::PullMergedAs
            | S::FilesChanged { .. }
            | S::CommentOpened
            | S::CommentCommented
            | S::ReviewApproved
            | S::ReviewChangesRequested
            | S::ReviewDismissed
            | S::Reviewed => {}
        }
    }
    let (number, author) = ("#7", "alice");
    let mut all = vec![
        CollabSentence::IssueSearchHelp,
        CollabSentence::PullSearchHelp,
        CollabSentence::RowOpened { number, author },
        CollabSentence::RowClosed { number },
        CollabSentence::RowClosedBy { number, author },
        CollabSentence::RowMergedBy { number, author },
        CollabSentence::NoOneAssignYourself,
        CollabSentence::PercentComplete,
        CollabSentence::WantsToMerge,
        CollabSentence::MergedInto,
        CollabSentence::ProposedToMerge,
        CollabSentence::SignInToOpenPull,
        CollabSentence::SignInToCommentOnLines,
        CollabSentence::SignInToJoin,
        CollabSentence::PullMergedAs,
        CollabSentence::CommentOpened,
        CollabSentence::CommentCommented,
        CollabSentence::ReviewApproved,
        CollabSentence::ReviewChangesRequested,
        CollabSentence::ReviewDismissed,
        CollabSentence::Reviewed,
    ];
    for count in COUNTS {
        all.extend([
            CollabSentence::OpenCount { count },
            CollabSentence::ClosedCount { count },
            CollabSentence::OpenedIssue { comments: count },
            CollabSentence::OpenedPull { comments: count },
            CollabSentence::FilesChanged { count },
        ]);
    }
    all.iter().copied().for_each(listed);
    all
}

fn events() -> Vec<TimelineEventMessage> {
    fn listed(message: TimelineEventMessage) {
        use TimelineEventMessage as E;
        match message {
            E::Closed { .. }
            | E::ClosedIn { .. }
            | E::Reopened
            | E::Renamed
            | E::Locked
            | E::Unlocked
            | E::Labeled
            | E::Unlabeled
            | E::Milestoned
            | E::Demilestoned
            | E::SelfAssigned
            | E::Assigned
            | E::SelfUnassigned
            | E::Unassigned
            | E::CommitReferenced
            | E::MentionedInIssue
            | E::MentionedInPull
            | E::Mentioned
            | E::ForcePushed
            | E::ForcePushedBranch
            | E::Pushed { .. }
            | E::PushedTo { .. }
            | E::BaseChanged
            | E::Merged
            | E::MergedInto
            | E::ReviewRequestedSelf
            | E::ReviewRequested
            | E::ReviewRequestRemoved
            | E::ReviewDismissedOf
            | E::ReviewDismissed
            | E::ThreadResolved
            | E::ThreadReopened
            | E::HeadRefDeleted
            | E::ConvertedToDraft
            | E::ReadyForReview => {}
        }
    }
    use TimelineEventMessage as E;
    let mut all = vec![
        E::Reopened,
        E::Renamed,
        E::Locked,
        E::Unlocked,
        E::Labeled,
        E::Unlabeled,
        E::Milestoned,
        E::Demilestoned,
        E::SelfAssigned,
        E::Assigned,
        E::SelfUnassigned,
        E::Unassigned,
        E::CommitReferenced,
        E::MentionedInIssue,
        E::MentionedInPull,
        E::Mentioned,
        E::ForcePushed,
        E::ForcePushedBranch,
        E::BaseChanged,
        E::Merged,
        E::MergedInto,
        E::ReviewRequestedSelf,
        E::ReviewRequested,
        E::ReviewRequestRemoved,
        E::ReviewDismissedOf,
        E::ReviewDismissed,
        E::ThreadResolved,
        E::ThreadReopened,
        E::HeadRefDeleted,
        E::ConvertedToDraft,
        E::ReadyForReview,
    ];
    for reason in [CloseReason::Completed, CloseReason::NotPlanned, CloseReason::Merged] {
        all.extend([E::Closed { reason }, E::ClosedIn { reason }]);
    }
    for count in COUNTS {
        all.extend([E::Pushed { count }, E::PushedTo { count }]);
    }
    all.iter().copied().for_each(listed);
    all
}

#[test]
fn every_rich_translation_keeps_every_slot_exactly_once() {
    let mismatches: Vec<String> = slot_mismatches(&sentences()).into_iter().chain(slot_mismatches(&events())).collect();
    assert!(mismatches.is_empty(), "translations that drop or repeat a slot:\n{}", mismatches.join("\n"));
}

#[test]
fn timeline_events_start_with_the_actor_in_every_locale() {
    for event in events() {
        for locale in LOCALES {
            assert!(slots(locale, event).contains(&0), "{event:?} in {locale:?} names no actor");
        }
    }
}

/// A test-only rich family whose German entry drops a slot and whose French entry repeats
/// one, to show the check above catches both.
#[derive(Clone, Copy, Debug)]
enum Broken {
    Moved,
}

impl RichFamily for Broken {
    fn id(self) -> &'static str {
        "test.broken"
    }

    fn english(self, text: &mut RichWriter) {
        text.slot(0);
        text.literal(" moved this to ");
        text.slot(1);
    }

    fn translate(self, locale: Locale, text: &mut RichWriter) -> bool {
        super::write(self, locale, text)
    }
}

entries! { De:
    Broken => RichWriter |text| {
        Broken::Moved => {
            text.slot(0);
            text.literal(" hat das verschoben");
        }
    }
}

entries! { Fr:
    Broken => RichWriter |text| {
        Broken::Moved => {
            text.slot(0);
            text.literal(" a déplacé ceci vers ");
            text.slot(0);
        }
    }
}

entries! { Ja:
    Broken => RichWriter |text| {
        Broken::Moved => {
            text.slot(1);
            text.literal("に");
            text.slot(0);
            text.literal("が移動しました");
        }
    }
}

entries! { Es: Broken => RichWriter |text| {} }
entries! { ZhHans: Broken => RichWriter |text| {} }
entries! { PtBr: Broken => RichWriter |text| {} }
entries! { Ru: Broken => RichWriter |text| {} }
entries! { Ko: Broken => RichWriter |text| {} }

#[test]
fn a_dropped_or_repeated_slot_is_reported_and_a_reordered_one_is_not() {
    let mismatches = slot_mismatches(&[Broken::Moved]);
    assert_eq!(mismatches.len(), 2, "{mismatches:#?}");
    assert!(mismatches[0].contains(" in de: slots [0], English [0, 1]"), "{}", mismatches[0]);
    assert!(mismatches[1].contains(" in fr: slots [0, 0], English [0, 1]"), "{}", mismatches[1]);
}
