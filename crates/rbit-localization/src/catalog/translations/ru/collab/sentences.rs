//! Russian (`ru`) entries for `catalog/collab/sentences.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::collab::sentences::CollabSentence;
use crate::plural::Plural;

entries! { Ru:
    CollabSentence<'a> => RichWriter |text| {
        CollabSentence::OpenCount { count } => {
            text.literal("Открыто: ");
            text.slot(0);
        }
        CollabSentence::ClosedCount { count } => {
            text.literal("Закрыто: ");
            text.slot(0);
        }
        CollabSentence::IssueSearchHelp => {
            text.literal("Фильтруйте результаты с помощью меню или вводите условия, например ");
            text.slot(0);
            text.literal(", ");
            text.slot(1);
            text.literal(", ");
            text.slot(2);
            text.literal(" или ");
            text.slot(3);
            text.literal(". Текст ищется в заголовках и описаниях. Каждое меню сохраняет остальные фильтры.");
        }
        CollabSentence::PullSearchHelp => {
            text.literal("Выберите фильтры ниже или введите условия, например ");
            text.slot(0);
            text.literal(", ");
            text.slot(1);
            text.literal(", ");
            text.slot(2);
            text.literal(" или ");
            text.slot(3);
            text.literal(".");
        }
        CollabSentence::OpenedIssue { comments } => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" открыл эту задачу ");
            text.slot(1);
            text.literal(" · ");
            text.number(comments);
            text.literal(" ");
            text.literal(match text.plural(comments) {
                Plural::One => "комментарий",
                Plural::Few => "комментария",
                _ => "комментариев",
            });
        }
        CollabSentence::OpenedPull { comments } => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" открыл этот запрос на слияние ");
            text.slot(1);
            text.literal(" · ");
            text.number(comments);
            text.literal(" ");
            text.literal(match text.plural(comments) {
                Plural::One => "комментарий",
                Plural::Few => "комментария",
                _ => "комментариев",
            });
        }
        CollabSentence::RowOpened { number, author } => {
            text.argument(number);
            text.literal(" · открыто ");
            text.slot(0);
            text.literal(" пользователем ");
            text.argument(author);
        }
        CollabSentence::RowClosed { number } => {
            text.argument(number);
            text.literal(" · закрыто ");
            text.slot(0);
        }
        CollabSentence::RowClosedBy { number, author } => {
            text.argument(number);
            text.literal(" · закрыто ");
            text.slot(0);
            text.literal(" пользователем ");
            text.argument(author);
        }
        CollabSentence::RowMergedBy { number, author } => {
            text.argument(number);
            text.literal(" · слито ");
            text.slot(0);
            text.literal(" пользователем ");
            text.argument(author);
        }
        CollabSentence::NoOneAssignYourself => {
            text.literal("Никого — ");
            text.slot(0);
        }
        CollabSentence::PercentComplete => {
            text.literal("Выполнено: ");
            text.slot(0);
        }
        CollabSentence::WantsToMerge => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" хочет слить ");
            text.slot(1);
            text.literal(" в ");
            text.slot(2);
            text.literal(" из ");
            text.slot(3);
        }
        CollabSentence::MergedInto => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" слил ");
            text.slot(1);
            text.literal(" в ");
            text.slot(2);
            text.literal(" из ");
            text.slot(3);
        }
        CollabSentence::ProposedToMerge => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" предложил слить ");
            text.slot(1);
            text.literal(" в ");
            text.slot(2);
            text.literal(" из ");
            text.slot(3);
        }
        CollabSentence::SignInToOpenPull => {
            text.slot(0);
            text.literal(", чтобы открыть запрос на слияние.");
        }
        CollabSentence::SignInToCommentOnLines => {
            text.slot(0);
            text.literal(", чтобы комментировать строки.");
        }
        CollabSentence::SignInToJoin => {
            text.slot(0);
            text.literal(", чтобы присоединиться к обсуждению.");
        }
        CollabSentence::PullMergedAs => {
            text.literal("Запрос на слияние слит коммитом ");
            text.slot(0);
            text.literal(".");
        }
        CollabSentence::FilesChanged { count } => {
            text.slot(0);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "файл изменён",
                Plural::Few => "файла изменено",
                _ => "файлов изменено",
            });
        }
        CollabSentence::CommentOpened => {
            text.literal("· открыто ");
            text.slot(0);
        }
        CollabSentence::CommentCommented => {
            text.literal("· комментарий ");
            text.slot(0);
        }
        CollabSentence::ReviewApproved => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" одобрил эти изменения");
        }
        CollabSentence::ReviewChangesRequested => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" запросил изменения");
        }
        CollabSentence::ReviewDismissed => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" оставил ревью (отклонено)");
        }
        CollabSentence::Reviewed => {
            text.literal("Пользователь ");
            text.slot(0);
            text.literal(" оставил ревью");
        }
    }
}
