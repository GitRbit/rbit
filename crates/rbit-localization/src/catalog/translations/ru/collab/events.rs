//! Russian (`ru`) entries for `catalog/collab/events.rs`.

use crate::catalog::collab::events::CloseReason;
use crate::catalog::collab::events::{TimelineEventMessage, TimelineNoteMessage};
use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Ru:
    TimelineEventMessage => RichWriter |text| {
        TimelineEventMessage::Closed { reason: CloseReason::Completed } => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" закрыл с отметкой «выполнено»");
        }
        TimelineEventMessage::Closed { reason: CloseReason::NotPlanned } => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" закрыл с отметкой «не запланировано»");
        }
        TimelineEventMessage::Closed { reason: CloseReason::Merged } => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" закрыл с отметкой «слито»");
        }
        TimelineEventMessage::ClosedIn { reason: CloseReason::Completed } => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" закрыл с отметкой «выполнено» в ");
            text.slot(1);
        }
        TimelineEventMessage::ClosedIn { reason: CloseReason::NotPlanned } => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" закрыл с отметкой «не запланировано» в ");
            text.slot(1);
        }
        TimelineEventMessage::ClosedIn { reason: CloseReason::Merged } => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" закрыл с отметкой «слито» в ");
            text.slot(1);
        }
        TimelineEventMessage::Reopened => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" открыл повторно");
        }
        TimelineEventMessage::Renamed => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" изменил заголовок с ");
            text.slot(1);
            text.literal(" на ");
            text.slot(2);
        }
        TimelineEventMessage::Locked => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" заблокировал обсуждение и ограничил его соавторами");
        }
        TimelineEventMessage::Unlocked => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" разблокировал обсуждение");
        }
        TimelineEventMessage::Labeled => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" добавил метку ");
            text.slot(1);
        }
        TimelineEventMessage::Unlabeled => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" удалил метку ");
            text.slot(1);
        }
        TimelineEventMessage::Milestoned => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" добавил в этап ");
            text.slot(1);
        }
        TimelineEventMessage::Demilestoned => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" удалил из этапа ");
            text.slot(1);
        }
        TimelineEventMessage::SelfAssigned => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" назначил себя исполнителем");
        }
        TimelineEventMessage::Assigned => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" назначил исполнителем ");
            text.slot(1);
        }
        TimelineEventMessage::SelfUnassigned => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" снял с себя назначение");
        }
        TimelineEventMessage::Unassigned => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" снял назначение с ");
            text.slot(1);
        }
        TimelineEventMessage::CommitReferenced => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" сослался на это в коммите ");
            text.slot(1);
        }
        TimelineEventMessage::MentionedInIssue => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" упомянул это в задаче ");
            text.slot(1);
        }
        TimelineEventMessage::MentionedInPull => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" упомянул это в запросе на слияние ");
            text.slot(1);
        }
        TimelineEventMessage::Mentioned => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" упомянул это");
        }
        TimelineEventMessage::ForcePushed => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" принудительно отправил изменения: с ");
            text.slot(2);
            text.literal(" на ");
            text.slot(3);
        }
        TimelineEventMessage::ForcePushedBranch => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" принудительно отправил ветку ");
            text.slot(1);
            text.literal(" с ");
            text.slot(2);
            text.literal(" на ");
            text.slot(3);
        }
        TimelineEventMessage::Pushed { count } => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" добавил ");
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "коммит",
                Plural::Few => "коммита",
                _ => "коммитов",
            });
            text.literal(", теперь на ");
            text.slot(2);
        }
        TimelineEventMessage::PushedTo { count } => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" добавил ");
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "коммит",
                Plural::Few => "коммита",
                _ => "коммитов",
            });
            text.literal(" в ");
            text.slot(1);
            text.literal(", теперь на ");
            text.slot(2);
        }
        TimelineEventMessage::BaseChanged => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" изменил базовую ветку с ");
            text.slot(1);
            text.literal(" на ");
            text.slot(2);
        }
        TimelineEventMessage::Merged => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" слил коммит ");
            text.slot(1);
            text.literal(" ");
            text.slot(3);
        }
        TimelineEventMessage::MergedInto => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" слил коммит ");
            text.slot(1);
            text.literal(" в ");
            text.slot(2);
            text.literal(" ");
            text.slot(3);
        }
        TimelineEventMessage::ReviewRequestedSelf => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" запросил ревью у себя");
        }
        TimelineEventMessage::ReviewRequested => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" запросил ревью у ");
            text.slot(1);
        }
        TimelineEventMessage::ReviewRequestRemoved => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" отменил запрос ревью у ");
            text.slot(1);
        }
        TimelineEventMessage::ReviewDismissedOf => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" отклонил ");
            text.slot(2);
            text.literal(" пользователя ");
            text.slot(1);
        }
        TimelineEventMessage::ReviewDismissed => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" отклонил ");
            text.slot(2);
        }
        TimelineEventMessage::ThreadResolved => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" отметил ");
            text.slot(1);
            text.literal(" как решённое");
        }
        TimelineEventMessage::ThreadReopened => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" повторно открыл ");
            text.slot(1);
        }
        TimelineEventMessage::HeadRefDeleted => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" удалил ветку ");
            text.slot(1);
        }
        TimelineEventMessage::ConvertedToDraft => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" перевёл этот запрос на слияние в черновик");
        }
        TimelineEventMessage::ReadyForReview => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" отметил этот запрос на слияние как готовый к ревью");
        }
    }
    TimelineNoteMessage => MessageWriter |text| {
        TimelineNoteMessage::Review => text.literal("ревью"),
        TimelineNoteMessage::ReviewConversation => text.literal("обсуждение ревью"),
        TimelineNoteMessage::MergeCommit => text.literal("(коммит слияния)"),
        TimelineNoteMessage::Squash => text.literal("(объединение коммитов)"),
        TimelineNoteMessage::Rebase => text.literal("(перебазирование)"),
        TimelineNoteMessage::OutsideInstance => text.literal("(вне этого сервера)"),
    }
}
