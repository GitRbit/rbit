//! German (`de`) entries for `catalog/collab/events.rs`.

use crate::catalog::collab::events::{CloseReason, TimelineEventMessage, TimelineNoteMessage};
use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { De:
    TimelineEventMessage => RichWriter |text| {
        TimelineEventMessage::Closed { reason } => {
            text.slot(0);
            text.literal(" hat dies ");
            closed(text, reason);
        }
        TimelineEventMessage::ClosedIn { reason } => {
            text.slot(0);
            then(text, " hat dies in ", 1);
            text.literal(" ");
            closed(text, reason);
        }
        TimelineEventMessage::Reopened => said(text, " hat dies erneut geöffnet"),
        TimelineEventMessage::Renamed => {
            text.slot(0);
            then(text, " hat den Titel von ", 1);
            then(text, " in ", 2);
            text.literal(" geändert");
        }
        TimelineEventMessage::Locked => {
            said(text, " hat die Unterhaltung gesperrt und auf Mitarbeitende beschränkt");
        }
        TimelineEventMessage::Unlocked => said(text, " hat die Unterhaltung entsperrt"),
        TimelineEventMessage::Labeled => around(text, " hat das Label ", 1, " hinzugefügt"),
        TimelineEventMessage::Unlabeled => around(text, " hat das Label ", 1, " entfernt"),
        TimelineEventMessage::Milestoned => around(text, " hat dies dem Meilenstein ", 1, " hinzugefügt"),
        TimelineEventMessage::Demilestoned => around(text, " hat dies aus dem Meilenstein ", 1, " entfernt"),
        TimelineEventMessage::SelfAssigned => said(text, " hat sich dies selbst zugewiesen"),
        TimelineEventMessage::Assigned => around(text, " hat ", 1, " zugewiesen"),
        TimelineEventMessage::SelfUnassigned => said(text, " hat die eigene Zuweisung entfernt"),
        TimelineEventMessage::Unassigned => around(text, " hat die Zuweisung von ", 1, " entfernt"),
        TimelineEventMessage::CommitReferenced => around(text, " hat dies in Commit ", 1, " referenziert"),
        TimelineEventMessage::MentionedInIssue => around(text, " hat dies in Issue ", 1, " erwähnt"),
        TimelineEventMessage::MentionedInPull => around(text, " hat dies in Pull-Request ", 1, " erwähnt"),
        TimelineEventMessage::Mentioned => said(text, " hat dies erwähnt"),
        TimelineEventMessage::ForcePushed => {
            text.slot(0);
            then(text, " hat einen Force-Push von ", 2);
            then(text, " auf ", 3);
            text.literal(" durchgeführt");
        }
        TimelineEventMessage::ForcePushedBranch => {
            text.slot(0);
            then(text, " hat einen Force-Push auf den Branch ", 1);
            then(text, " von ", 2);
            then(text, " auf ", 3);
            text.literal(" durchgeführt");
        }
        TimelineEventMessage::Pushed { count } => pushed(text, count, false),
        TimelineEventMessage::PushedTo { count } => pushed(text, count, true),
        TimelineEventMessage::BaseChanged => {
            text.slot(0);
            then(text, " hat den Ziel-Branch von ", 1);
            then(text, " in ", 2);
            text.literal(" geändert");
        }
        TimelineEventMessage::Merged => {
            text.slot(0);
            then(text, " hat Commit ", 1);
            then(text, " zusammengeführt ", 3);
        }
        TimelineEventMessage::MergedInto => {
            text.slot(0);
            then(text, " hat Commit ", 1);
            then(text, " in ", 2);
            then(text, " zusammengeführt ", 3);
        }
        TimelineEventMessage::ReviewRequestedSelf => said(text, " hat ein Review von sich selbst angefordert"),
        TimelineEventMessage::ReviewRequested => around(text, " hat ein Review von ", 1, " angefordert"),
        TimelineEventMessage::ReviewRequestRemoved => around(text, " hat die Review-Anfrage an ", 1, " entfernt"),
        TimelineEventMessage::ReviewDismissedOf => {
            text.slot(0);
            then(text, " hat das ", 2);
            then(text, " von ", 1);
            text.literal(" verworfen");
        }
        TimelineEventMessage::ReviewDismissed => around(text, " hat ein ", 2, " verworfen"),
        TimelineEventMessage::ThreadResolved => around(text, " hat ", 1, " als gelöst markiert"),
        TimelineEventMessage::ThreadReopened => around(text, " hat ", 1, " erneut geöffnet"),
        TimelineEventMessage::HeadRefDeleted => around(text, " hat den Branch ", 1, " gelöscht"),
        TimelineEventMessage::ConvertedToDraft => said(text, " hat diesen Pull-Request als Entwurf markiert"),
        TimelineEventMessage::ReadyForReview => {
            said(text, " hat diesen Pull-Request als bereit für ein Review markiert");
        }
    }
    TimelineNoteMessage => MessageWriter |text| {
        TimelineNoteMessage::Review => text.literal("Review"),
        TimelineNoteMessage::ReviewConversation => text.literal("eine Review-Unterhaltung"),
        TimelineNoteMessage::MergeCommit => text.literal("(Merge-Commit)"),
        TimelineNoteMessage::Squash => text.literal("(Squash)"),
        TimelineNoteMessage::Rebase => text.literal("(Rebase)"),
        TimelineNoteMessage::OutsideInstance => text.literal("(außerhalb dieser Instanz)"),
    }
}

/// The actor, then `words`.
fn said(text: &mut RichWriter, words: &str) {
    text.slot(0);
    text.literal(words);
}

/// The actor, `before`, slot `slot`, then `after`.
fn around(text: &mut RichWriter, before: &str, slot: usize, after: &str) {
    text.slot(0);
    then(text, before, slot);
    text.literal(after);
}

/// `words`, then slot `slot`.
fn then(text: &mut RichWriter, words: &str, slot: usize) {
    text.literal(words);
    text.slot(slot);
}

fn closed(text: &mut RichWriter, reason: CloseReason) {
    text.literal(match reason {
        CloseReason::Completed => "als erledigt geschlossen",
        CloseReason::NotPlanned => "als nicht geplant geschlossen",
        CloseReason::Merged => "als zusammengeführt geschlossen",
    });
}

/// „hat 2 Commits zu main hinzugefügt, jetzt bei abc1234“.
fn pushed(text: &mut RichWriter, count: u64, to_branch: bool) {
    text.slot(0);
    text.literal(" hat ");
    text.number(count);
    text.literal(match text.plural(count) {
        Plural::One => " Commit",
        _ => " Commits",
    });
    if to_branch {
        then(text, " zu ", 1);
    }
    then(text, " hinzugefügt, jetzt bei ", 2);
}
