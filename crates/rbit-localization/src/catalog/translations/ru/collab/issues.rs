//! Russian (`ru`) entries for `catalog/collab/issues.rs`.

use crate::catalog::collab::issues::IssueMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Ru:
    IssueMessage<'a> => MessageWriter |text| {
        IssueMessage::Issues => text.literal("Задачи"),
        IssueMessage::Labels => text.literal("Метки"),
        IssueMessage::Milestones => text.literal("Этапы"),
        IssueMessage::NewIssue => text.literal("Новая задача"),
        IssueMessage::SearchIssues => text.literal("Поиск задач"),
        IssueMessage::SearchPlaceholder => text.literal("Поиск по заголовкам и описаниям…"),
        IssueMessage::Search => text.literal("Поиск"),
        IssueMessage::SearchHelp => text.literal("Справка по поиску"),
        IssueMessage::ClearSearch => text.literal("Сбросить поиск, фильтры и сортировку"),
        IssueMessage::StateNav => text.literal("Состояние"),
        IssueMessage::NoIssuesTitle => text.literal("Подходящих задач нет"),
        IssueMessage::NoIssuesText => text.literal("Попробуйте другой запрос или создайте первую задачу."),
        IssueMessage::DescriptionHint => text.literal("Markdown; #12 и @name становятся ссылками"),
        IssueMessage::NoLabelsYet => text.literal("Меток пока нет."),
        IssueMessage::Assignees => text.literal("Исполнители"),
        IssueMessage::Milestone => text.literal("Этап"),
        IssueMessage::NoMilestone => text.literal("Без этапа"),
        IssueMessage::OpenIssue => text.literal("Создать задачу"),
        IssueMessage::Details => text.literal("Подробности"),
        IssueMessage::Locked => text.literal("Заблокировано"),
        IssueMessage::AssignedTo { names } => {
            text.literal("Исполнители: ");
            text.argument(names);
        }
        IssueMessage::Comments { count } => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "комментарий",
                Plural::Few => "комментария",
                _ => "комментариев",
            });
        }
    }
}
