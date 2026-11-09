//! French (`fr`) entries for `catalog/collab/events.rs`.

use crate::catalog::collab::events::{CloseReason, TimelineEventMessage, TimelineNoteMessage};
use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Fr:
    TimelineEventMessage => RichWriter |text| {
        TimelineEventMessage::Closed { reason } => {
            text.slot(0);
            text.literal(closed(reason));
        }
        TimelineEventMessage::ClosedIn { reason } => {
            text.slot(0);
            text.literal(closed(reason));
            then(text, " dans ", 1);
        }
        TimelineEventMessage::Reopened => actor(text, " a rouvert ceci"),
        TimelineEventMessage::Renamed => {
            actor_then(text, " a modifié le titre ", 1);
            then(text, " ", 2);
        }
        TimelineEventMessage::Locked => actor(text, " a verrouillé la conversation et l’a limitée aux collaborateurs"),
        TimelineEventMessage::Unlocked => actor(text, " a déverrouillé cette conversation"),
        TimelineEventMessage::Labeled => actor_then(text, " a ajouté l’étiquette ", 1),
        TimelineEventMessage::Unlabeled => actor_then(text, " a retiré l’étiquette ", 1),
        TimelineEventMessage::Milestoned => actor_then(text, " a ajouté ceci au jalon ", 1),
        TimelineEventMessage::Demilestoned => actor_then(text, " a retiré ceci du jalon ", 1),
        TimelineEventMessage::SelfAssigned => actor(text, " s’est assigné ceci"),
        TimelineEventMessage::Assigned => actor_then(text, " a assigné ", 1),
        TimelineEventMessage::SelfUnassigned => actor(text, " a retiré son assignation"),
        TimelineEventMessage::Unassigned => actor_then(text, " a retiré l’assignation de ", 1),
        TimelineEventMessage::CommitReferenced => actor_then(text, " a référencé ceci dans le commit ", 1),
        TimelineEventMessage::MentionedInIssue => actor_then(text, " a mentionné ceci dans le ticket ", 1),
        TimelineEventMessage::MentionedInPull => actor_then(text, " a mentionné ceci dans la pull request ", 1),
        TimelineEventMessage::Mentioned => actor(text, " a mentionné ceci"),
        TimelineEventMessage::ForcePushed => {
            actor_then(text, " a effectué un push forcé de ", 2);
            then(text, " vers ", 3);
        }
        TimelineEventMessage::ForcePushedBranch => {
            actor_then(text, " a effectué un push forcé sur la branche ", 1);
            then(text, " de ", 2);
            then(text, " vers ", 3);
        }
        TimelineEventMessage::Pushed { count } => {
            pushed(text, count);
            then(text, ", désormais sur ", 2);
        }
        TimelineEventMessage::PushedTo { count } => {
            pushed(text, count);
            then(text, " à ", 1);
            then(text, ", désormais sur ", 2);
        }
        TimelineEventMessage::BaseChanged => {
            actor_then(text, " a changé la branche de base de ", 1);
            then(text, " en ", 2);
        }
        TimelineEventMessage::Merged => {
            actor_then(text, " a fusionné le commit ", 1);
            then(text, " ", 3);
        }
        TimelineEventMessage::MergedInto => {
            actor_then(text, " a fusionné le commit ", 1);
            then(text, " dans ", 2);
            then(text, " ", 3);
        }
        TimelineEventMessage::ReviewRequestedSelf => actor(text, " s’est demandé une revue"),
        TimelineEventMessage::ReviewRequested => actor_then(text, " a demandé une revue à ", 1),
        TimelineEventMessage::ReviewRequestRemoved => actor_then(text, " a retiré la demande de revue adressée à ", 1),
        TimelineEventMessage::ReviewDismissedOf => {
            actor_then(text, " a rejeté la ", 2);
            then(text, " de ", 1);
        }
        TimelineEventMessage::ReviewDismissed => actor_then(text, " a rejeté une ", 2),
        TimelineEventMessage::ThreadResolved => actor_then(text, " a résolu ", 1),
        TimelineEventMessage::ThreadReopened => actor_then(text, " a rouvert ", 1),
        TimelineEventMessage::HeadRefDeleted => actor_then(text, " a supprimé la branche ", 1),
        TimelineEventMessage::ConvertedToDraft => actor(text, " a marqué cette pull request comme brouillon"),
        TimelineEventMessage::ReadyForReview => actor(text, " a marqué cette pull request comme prête pour la revue"),
    }
    TimelineNoteMessage => MessageWriter |text| {
        TimelineNoteMessage::Review => text.literal("revue"),
        TimelineNoteMessage::ReviewConversation => text.literal("une conversation de revue"),
        TimelineNoteMessage::MergeCommit => text.literal("(commit de fusion)"),
        TimelineNoteMessage::Squash => text.literal("(squash)"),
        TimelineNoteMessage::Rebase => text.literal("(rebase)"),
        TimelineNoteMessage::OutsideInstance => text.literal("(hors de cette instance)"),
    }
}

const fn closed(reason: CloseReason) -> &'static str {
    match reason {
        CloseReason::Completed => " a fermé ceci comme terminé",
        CloseReason::NotPlanned => " a fermé ceci comme non prévu",
        CloseReason::Merged => " a fermé ceci comme fusionné",
    }
}

/// The actor (slot 0), then `words`.
fn actor(text: &mut RichWriter, words: &str) {
    text.slot(0);
    text.literal(words);
}

/// The actor (slot 0), `words`, then slot `slot`.
fn actor_then(text: &mut RichWriter, words: &str, slot: usize) {
    text.slot(0);
    then(text, words, slot);
}

/// `words`, then slot `slot`.
fn then(text: &mut RichWriter, words: &str, slot: usize) {
    text.literal(words);
    text.slot(slot);
}

/// « alice a ajouté 3 commits ».
fn pushed(text: &mut RichWriter, count: u64) {
    text.slot(0);
    text.literal(" a ajouté ");
    text.number(count);
    text.literal(match text.plural(count) {
        Plural::One => " commit",
        _ => " commits",
    });
}
