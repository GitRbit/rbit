//! Issue, pull request and timeline families: English everywhere, marked as a fallback
//! in Spanish (which has no translation yet), never in the diagnostic catalogs.

use std::collections::HashSet;

use rbit_localization::{
    ChecksMessage, CloseReason, CollabSentence, IssueFilterMenu, IssueFilterMessage, IssueMessage, IssueSidebarMessage, LabelMessage,
    Locale, Localized, MergeMessage, MessageFamily, MilestoneMessage, PullMessage, ReviewMessage, Rich, RichFamily, ThreadMessage,
    TimelineEventMessage, TimelineNoteMessage,
};

/// One sample of every plain family: (id, English, Spanish, pseudo-expanded).
fn plain() -> Vec<(&'static str, Localized, Localized, Localized)> {
    fn sample<M: MessageFamily>(message: M, localized: fn(M, Locale) -> Localized) -> (&'static str, Localized, Localized, Localized) {
        (message.id(), localized(message, Locale::English), localized(message, Locale::Spanish), localized(message, Locale::PseudoExpanded))
    }
    vec![
        sample(IssueMessage::NewIssue, IssueMessage::localized),
        sample(IssueMessage::Comments { count: 1 }, IssueMessage::localized),
        sample(IssueFilterMessage::Find(IssueFilterMenu::Author), IssueFilterMessage::localized),
        sample(IssueSidebarMessage::ReasonAssigned, IssueSidebarMessage::localized),
        sample(LabelMessage::Count { count: 9 }, LabelMessage::localized),
        sample(MilestoneMessage::PastDueBy { due: "Jan 5, 2030" }, MilestoneMessage::localized),
        sample(PullMessage::CommitsPhrase { count: Some(3) }, PullMessage::localized),
        sample(ReviewMessage::OutdatedComments { count: 1 }, ReviewMessage::localized),
        sample(MergeMessage::Approvals { have: 0, need: 1 }, MergeMessage::localized),
        sample(ThreadMessage::LockConversation, ThreadMessage::localized),
        sample(TimelineNoteMessage::Squash, TimelineNoteMessage::localized),
        sample(ChecksMessage::Summary { successful: 1, failing: 2, pending: 1, expected: 1 }, ChecksMessage::localized),
    ]
}

#[test]
fn plain_families_render_english_and_fall_back_in_spanish() {
    let expected = [
        "New issue",
        "1 comment",
        "Find author",
        "You are assigned.",
        "9 labels",
        "Past due by Jan 5, 2030",
        "3 commits",
        "1 outdated comment is in the conversation",
        "0 of 1 required approval",
        "Lock conversation",
        "(squash)",
        "1 successful, 2 failing and 1 pending checks, 1 expected",
    ];
    for ((id, english, spanish, pseudo), text) in plain().into_iter().zip(expected) {
        assert_eq!((english.as_str(), english.fallback()), (text, false), "{id}");
        assert_eq!((spanish.as_str(), spanish.fallback()), (text, true), "{id} falls back to English");
        assert!(!pseudo.fallback() && pseudo.as_str().starts_with('[') && pseudo.as_str() != text, "{id} is expanded");
    }
}

#[test]
fn arguments_are_kept_as_plain_text() {
    let confirm = LabelMessage::DeleteConfirm { name: "<b>bug</b>" }.localized(Locale::Spanish);
    assert_eq!(confirm.as_str(), "Delete the label “<b>bug</b>”? It is removed from every issue.");
    let rtl = PullMessage::DeleteBranch { branch: "bob:fix" }.localized(Locale::PseudoRtl);
    assert!(rtl.as_str().contains("\u{2068}bob:fix\u{2069}"), "{rtl}");
}

fn sentence(rich: &Rich, slots: &[&str]) -> String {
    rich.plain(slots)
}

#[test]
fn rich_sentences_name_their_slots() {
    let header = CollabSentence::WantsToMerge.rich(Locale::English);
    assert!(!header.fallback());
    assert_eq!(sentence(&header, &["bob", "2 commits", "main", "bob:fix"]), "bob wants to merge 2 commits into main from bob:fix");
    let slots: Vec<usize> = header.parts().iter().filter_map(rbit_localization::RichPart::slot).collect();
    assert_eq!(slots, [0, 1, 2, 3]);

    let spanish = CollabSentence::RowOpened { number: "#12", author: "alice" }.rich(Locale::Spanish);
    assert!(spanish.fallback(), "no Spanish translation yet");
    assert_eq!(sentence(&spanish, &["2 days ago"]), "#12 opened 2 days ago by alice");
    assert_eq!(CollabSentence::FilesChanged { count: 1 }.rich(Locale::English).plain(&["1"]), "1 file changed");

    let rtl = CollabSentence::OpenCount { count: 3 }.rich(Locale::PseudoRtl);
    assert_eq!(rtl.plain(&["3"]), "[\u{2068}3\u{2069} Open]", "slots are isolated, the sentence bracketed");
}

#[test]
fn timeline_events_start_with_their_actor() {
    let cases = [
        (TimelineEventMessage::Labeled, vec!["alice", "bug"], "alice added the bug label"),
        (
            TimelineEventMessage::ClosedIn { reason: CloseReason::Completed },
            vec!["alice", "abc1234"],
            "alice closed this as completed in abc1234",
        ),
        (
            TimelineEventMessage::PushedTo { count: 1 },
            vec!["alice", "feature", "def5678"],
            "alice added 1 commit to feature, now at def5678",
        ),
        (TimelineEventMessage::Pushed { count: 2 }, vec!["alice", "", "def5678"], "alice added 2 commits, now at def5678"),
        (TimelineEventMessage::ReviewDismissedOf, vec!["alice", "bob", "review"], "alice dismissed bob’s review"),
        (TimelineEventMessage::MergedInto, vec!["alice", "abc1234", "main", "(squash)"], "alice merged commit abc1234 into main (squash)"),
    ];
    for (event, slots, expected) in cases {
        let english = event.rich(Locale::English);
        assert_eq!((english.plain(&slots), english.fallback()), (expected.to_owned(), false), "{}", event.id());
        assert_eq!(english.parts()[0].slot(), Some(0), "{} starts with the actor", event.id());
        let spanish = event.rich(Locale::Spanish);
        assert_eq!((spanish.plain(&slots), spanish.fallback()), (expected.to_owned(), true));
    }
}

#[test]
fn ids_are_unique_and_namespaced() {
    let ids = [
        IssueMessage::Issues.id(),
        IssueMessage::NewIssue.id(),
        IssueFilterMessage::Menu(IssueFilterMenu::Sort).id(),
        IssueFilterMessage::Find(IssueFilterMenu::Sort).id(),
        IssueFilterMessage::FindPlaceholder(IssueFilterMenu::Sort).id(),
        IssueSidebarMessage::Assignees.id(),
        LabelMessage::NewLabel.id(),
        MilestoneMessage::NewMilestone.id(),
        PullMessage::PullRequests.id(),
        ReviewMessage::Approved.id(),
        MergeMessage::MergePull.id(),
        ThreadMessage::Open.id(),
        TimelineNoteMessage::Review.id(),
        CollabSentence::OpenCount { count: 0 }.id(),
        CollabSentence::ClosedCount { count: 0 }.id(),
        TimelineEventMessage::Locked.id(),
    ];
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
    for id in ids {
        assert!(
            ["issues.", "labels.", "milestones.", "pulls.", "reviews.", "merge.", "timeline."].iter().any(|prefix| id.starts_with(prefix)),
            "{id}"
        );
    }
}
