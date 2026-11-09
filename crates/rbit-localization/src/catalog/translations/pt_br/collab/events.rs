//! Brazilian Portuguese (`pt-BR`) entries for `catalog/collab/events.rs`.

use crate::catalog::collab::events::{CloseReason, TimelineEventMessage, TimelineNoteMessage};
use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { PtBr:
    TimelineEventMessage => RichWriter |text| {
        TimelineEventMessage::Closed { reason } => {
            text.slot(0);
            text.literal(closed(reason));
        }
        TimelineEventMessage::ClosedIn { reason } => {
            text.slot(0);
            text.literal(closed(reason));
            then(text, " em ", 1);
        }
        TimelineEventMessage::Reopened => actor(text, " reabriu isto"),
        TimelineEventMessage::Renamed => {
            text.slot(0);
            then(text, " alterou o título de ", 1);
            then(text, " para ", 2);
        }
        TimelineEventMessage::Locked => actor(text, " bloqueou a conversa e a limitou a colaboradores"),
        TimelineEventMessage::Unlocked => actor(text, " desbloqueou esta conversa"),
        TimelineEventMessage::Labeled => {
            text.slot(0);
            then(text, " adicionou a etiqueta ", 1);
        }
        TimelineEventMessage::Unlabeled => {
            text.slot(0);
            then(text, " removeu a etiqueta ", 1);
        }
        TimelineEventMessage::Milestoned => {
            text.slot(0);
            then(text, " adicionou isto ao marco ", 1);
        }
        TimelineEventMessage::Demilestoned => {
            text.slot(0);
            then(text, " removeu isto do marco ", 1);
        }
        TimelineEventMessage::SelfAssigned => actor(text, " atribuiu isto a si mesmo"),
        TimelineEventMessage::Assigned => {
            text.slot(0);
            then(text, " atribuiu isto a ", 1);
        }
        TimelineEventMessage::SelfUnassigned => actor(text, " removeu a própria atribuição"),
        TimelineEventMessage::Unassigned => {
            text.slot(0);
            then(text, " removeu a atribuição de ", 1);
        }
        TimelineEventMessage::CommitReferenced => {
            text.slot(0);
            then(text, " referenciou isto no commit ", 1);
        }
        TimelineEventMessage::MentionedInIssue => {
            text.slot(0);
            then(text, " mencionou isto na issue ", 1);
        }
        TimelineEventMessage::MentionedInPull => {
            text.slot(0);
            then(text, " mencionou isto no pull request ", 1);
        }
        TimelineEventMessage::Mentioned => actor(text, " mencionou isto"),
        TimelineEventMessage::ForcePushed => {
            text.slot(0);
            then(text, " fez force-push de ", 2);
            then(text, " para ", 3);
        }
        TimelineEventMessage::ForcePushedBranch => {
            text.slot(0);
            then(text, " fez force-push do branch ", 1);
            then(text, " de ", 2);
            then(text, " para ", 3);
        }
        TimelineEventMessage::Pushed { count } => {
            text.slot(0);
            pushed(text, count);
            then(text, ", agora em ", 2);
        }
        TimelineEventMessage::PushedTo { count } => {
            text.slot(0);
            pushed(text, count);
            then(text, " ao branch ", 1);
            then(text, ", agora em ", 2);
        }
        TimelineEventMessage::BaseChanged => {
            text.slot(0);
            then(text, " alterou o branch base de ", 1);
            then(text, " para ", 2);
        }
        TimelineEventMessage::Merged => {
            text.slot(0);
            then(text, " mesclou o commit ", 1);
            then(text, " ", 3);
        }
        TimelineEventMessage::MergedInto => {
            text.slot(0);
            then(text, " mesclou o commit ", 1);
            then(text, " em ", 2);
            then(text, " ", 3);
        }
        TimelineEventMessage::ReviewRequestedSelf => actor(text, " solicitou uma revisão a si mesmo"),
        TimelineEventMessage::ReviewRequested => {
            text.slot(0);
            then(text, " solicitou uma revisão de ", 1);
        }
        TimelineEventMessage::ReviewRequestRemoved => {
            text.slot(0);
            then(text, " removeu a solicitação de revisão de ", 1);
        }
        TimelineEventMessage::ReviewDismissedOf => {
            text.slot(0);
            then(text, " descartou a ", 2);
            then(text, " de ", 1);
        }
        TimelineEventMessage::ReviewDismissed => {
            text.slot(0);
            then(text, " descartou uma ", 2);
        }
        TimelineEventMessage::ThreadResolved => {
            text.slot(0);
            then(text, " resolveu ", 1);
        }
        TimelineEventMessage::ThreadReopened => {
            text.slot(0);
            then(text, " reabriu ", 1);
        }
        TimelineEventMessage::HeadRefDeleted => {
            text.slot(0);
            then(text, " excluiu o branch ", 1);
        }
        TimelineEventMessage::ConvertedToDraft => actor(text, " marcou este pull request como rascunho"),
        TimelineEventMessage::ReadyForReview => {
            actor(text, " marcou este pull request como pronto para revisão");
        }
    }
    TimelineNoteMessage => MessageWriter |text| {
        TimelineNoteMessage::Review => text.literal("revisão"),
        TimelineNoteMessage::ReviewConversation => text.literal("uma conversa de revisão"),
        TimelineNoteMessage::MergeCommit => text.literal("(commit de mesclagem)"),
        TimelineNoteMessage::Squash => text.literal("(squash)"),
        TimelineNoteMessage::Rebase => text.literal("(rebase)"),
        TimelineNoteMessage::OutsideInstance => text.literal("(fora desta instância)"),
    }
}

/// The actor (slot 0), then `words`.
fn actor(text: &mut RichWriter, words: &str) {
    text.slot(0);
    text.literal(words);
}

/// `words`, then slot `slot`.
fn then(text: &mut RichWriter, words: &str, slot: usize) {
    text.literal(words);
    text.slot(slot);
}

const fn closed(reason: CloseReason) -> &'static str {
    match reason {
        CloseReason::Completed => " fechou isto como concluído",
        CloseReason::NotPlanned => " fechou isto como não planejado",
        CloseReason::Merged => " fechou isto como mesclado",
    }
}

/// “ adicionou 3 commits”.
fn pushed(text: &mut RichWriter, count: u64) {
    text.literal(" adicionou ");
    text.number(count);
    text.literal(match text.plural(count) {
        Plural::One => " commit",
        _ => " commits",
    });
}
