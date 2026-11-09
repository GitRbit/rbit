//! Russian (`ru`) entries for `catalog/collab/milestones.rs`.

use crate::catalog::collab::milestones::MilestoneMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ru:
    MilestoneMessage<'a> => MessageWriter |text| {
        MilestoneMessage::NewMilestone => text.literal("Новый этап"),
        MilestoneMessage::EditMilestone => text.literal("Редактирование этапа"),
        MilestoneMessage::EmptyTitle => text.literal("Этапов здесь нет"),
        MilestoneMessage::EmptyText => text.literal("Этапы объединяют задачи для релиза или к определённой дате и показывают, насколько продвинулась работа."),
        MilestoneMessage::ClosedWhen { when } => {
            text.literal("Закрыт ");
            text.argument(when);
        }
        MilestoneMessage::DueBy { due } => {
            text.literal("Срок: ");
            text.argument(due);
        }
        MilestoneMessage::PastDueBy { due } => {
            text.literal("Просрочен, срок: ");
            text.argument(due);
        }
        MilestoneMessage::NoDueDate => text.literal("Без срока"),
        MilestoneMessage::Close => text.literal("Закрыть"),
        MilestoneMessage::Reopen => text.literal("Открыть повторно"),
        MilestoneMessage::DeleteConfirm { title } => {
            text.literal("Удалить этап «");
            text.argument(title);
            text.literal("»? Его задачи сохранятся.");
        }
        MilestoneMessage::SearchIssues => text.literal("Поиск по задачам этапа"),
        MilestoneMessage::NothingHereTitle => text.literal("Здесь ничего нет"),
        MilestoneMessage::NothingHereText => text.literal("Здесь появятся задачи и запросы на слияние, добавленные в этот этап."),
        MilestoneMessage::FormIntro => text.literal("Объединяйте задачи для релиза или к определённой дате."),
        MilestoneMessage::DueDate => text.literal("Срок"),
        MilestoneMessage::MarkdownOptional => text.literal("Markdown, необязательно"),
        MilestoneMessage::CreateMilestone => text.literal("Создать этап"),
        MilestoneMessage::SaveMilestone => text.literal("Сохранить этап"),
        MilestoneMessage::Complete { percent } => {
            text.literal("Выполнено: ");
            text.number(percent);
            text.literal("%");
        }
        MilestoneMessage::OpenIssues { count } => {
            text.literal("Открыто: ");
            text.number(count);
        }
        MilestoneMessage::ClosedIssues { count } => {
            text.literal("Закрыто: ");
            text.number(count);
        }
    }
}
