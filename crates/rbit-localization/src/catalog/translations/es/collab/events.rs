//! Spanish (`es`) entries for `catalog/collab/events.rs`.

use crate::catalog::collab::events::{CloseReason, TimelineEventMessage, TimelineNoteMessage};
use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Es:
    TimelineEventMessage => RichWriter |text| {
        TimelineEventMessage::Closed { reason } => {
            text.slot(0);
            closed(text, reason);
        }
        TimelineEventMessage::ClosedIn { reason } => {
            text.slot(0);
            closed(text, reason);
            then(text, " en ", 1);
        }
        TimelineEventMessage::Reopened => sentence(text, " reabrió esto"),
        TimelineEventMessage::Renamed => {
            text.slot(0);
            then(text, " cambió el título ", 1);
            then(text, " por ", 2);
        }
        TimelineEventMessage::Locked => {
            sentence(text, " bloqueó la conversación y la limitó a los colaboradores");
        }
        TimelineEventMessage::Unlocked => sentence(text, " desbloqueó esta conversación"),
        TimelineEventMessage::Labeled => {
            text.slot(0);
            then(text, " añadió la etiqueta ", 1);
        }
        TimelineEventMessage::Unlabeled => {
            text.slot(0);
            then(text, " quitó la etiqueta ", 1);
        }
        TimelineEventMessage::Milestoned => {
            text.slot(0);
            then(text, " añadió esto al hito ", 1);
        }
        TimelineEventMessage::Demilestoned => {
            text.slot(0);
            then(text, " quitó esto del hito ", 1);
        }
        TimelineEventMessage::SelfAssigned => sentence(text, " se asignó esto"),
        TimelineEventMessage::Assigned => {
            text.slot(0);
            then(text, " asignó a ", 1);
        }
        TimelineEventMessage::SelfUnassigned => sentence(text, " quitó su asignación"),
        TimelineEventMessage::Unassigned => {
            text.slot(0);
            then(text, " quitó la asignación a ", 1);
        }
        TimelineEventMessage::CommitReferenced => {
            text.slot(0);
            then(text, " hizo referencia a esto en el commit ", 1);
        }
        TimelineEventMessage::MentionedInIssue => {
            text.slot(0);
            then(text, " mencionó esto en la incidencia ", 1);
        }
        TimelineEventMessage::MentionedInPull => {
            text.slot(0);
            then(text, " mencionó esto en la solicitud de cambios ", 1);
        }
        TimelineEventMessage::Mentioned => sentence(text, " mencionó esto"),
        TimelineEventMessage::ForcePushed => {
            text.slot(0);
            then(text, " hizo un envío forzado de ", 2);
            then(text, " a ", 3);
        }
        TimelineEventMessage::ForcePushedBranch => {
            text.slot(0);
            then(text, " hizo un envío forzado a la rama ", 1);
            then(text, " de ", 2);
            then(text, " a ", 3);
        }
        TimelineEventMessage::Pushed { count } => {
            text.slot(0);
            added_commits(text, count);
            then(text, ", ahora en ", 2);
        }
        TimelineEventMessage::PushedTo { count } => {
            text.slot(0);
            added_commits(text, count);
            then(text, " a ", 1);
            then(text, ", ahora en ", 2);
        }
        TimelineEventMessage::BaseChanged => {
            text.slot(0);
            then(text, " cambió la rama base de ", 1);
            then(text, " a ", 2);
        }
        TimelineEventMessage::Merged => {
            text.slot(0);
            then(text, " fusionó el commit ", 1);
            then(text, " ", 3);
        }
        TimelineEventMessage::MergedInto => {
            text.slot(0);
            then(text, " fusionó el commit ", 1);
            then(text, " en ", 2);
            then(text, " ", 3);
        }
        TimelineEventMessage::ReviewRequestedSelf => sentence(text, " se solicitó una revisión a sí mismo"),
        TimelineEventMessage::ReviewRequested => {
            text.slot(0);
            then(text, " solicitó una revisión a ", 1);
        }
        TimelineEventMessage::ReviewRequestRemoved => {
            text.slot(0);
            then(text, " retiró la solicitud de revisión a ", 1);
        }
        TimelineEventMessage::ReviewDismissedOf => {
            text.slot(0);
            then(text, " descartó la ", 2);
            then(text, " de ", 1);
        }
        TimelineEventMessage::ReviewDismissed => {
            text.slot(0);
            then(text, " descartó una ", 2);
        }
        TimelineEventMessage::ThreadResolved => {
            text.slot(0);
            then(text, " resolvió ", 1);
        }
        TimelineEventMessage::ThreadReopened => {
            text.slot(0);
            then(text, " reabrió ", 1);
        }
        TimelineEventMessage::HeadRefDeleted => {
            text.slot(0);
            then(text, " eliminó la rama ", 1);
        }
        TimelineEventMessage::ConvertedToDraft => {
            sentence(text, " marcó esta solicitud de cambios como borrador");
        }
        TimelineEventMessage::ReadyForReview => {
            sentence(text, " marcó esta solicitud de cambios como lista para revisión");
        }
    }
    TimelineNoteMessage => MessageWriter |text| {
        TimelineNoteMessage::Review => text.literal("revisión"),
        TimelineNoteMessage::ReviewConversation => text.literal("una conversación de revisión"),
        TimelineNoteMessage::MergeCommit => text.literal("(commit de fusión)"),
        TimelineNoteMessage::Squash => text.literal("(squash)"),
        TimelineNoteMessage::Rebase => text.literal("(rebase)"),
        TimelineNoteMessage::OutsideInstance => text.literal("(fuera de esta instancia)"),
    }
}

/// The actor, then `words`.
fn sentence(text: &mut RichWriter, words: &str) {
    text.slot(0);
    text.literal(words);
}

/// `words`, then slot `slot`.
fn then(text: &mut RichWriter, words: &str, slot: usize) {
    text.literal(words);
    text.slot(slot);
}

fn closed(text: &mut RichWriter, reason: CloseReason) {
    text.literal(match reason {
        CloseReason::Completed => " cerró esto como completado",
        CloseReason::NotPlanned => " cerró esto como no planificado",
        CloseReason::Merged => " cerró esto como fusionado",
    });
}

/// " añadió 3 commits".
fn added_commits(text: &mut RichWriter, count: u64) {
    text.literal(" añadió ");
    text.number(count);
    text.literal(match text.plural(count) {
        Plural::One => " commit",
        _ => " commits",
    });
}
