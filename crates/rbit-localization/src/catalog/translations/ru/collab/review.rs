//! Russian (`ru`) entries for `catalog/collab/review.rs`.

use crate::catalog::collab::review::ReviewMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Ru:
    ReviewMessage<'a> => MessageWriter |text| {
        ReviewMessage::Changes => text.literal("Изменения"),
        ReviewMessage::ToggleFileTree => text.literal("Показать или скрыть дерево файлов"),
        ReviewMessage::CollapseAll => text.literal("Свернуть все"),
        ReviewMessage::ExpandAll => text.literal("Развернуть все"),
        ReviewMessage::OutdatedComments { count } => {
            text.literal("В обсуждении: ");
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "устаревший комментарий",
                Plural::Few => "устаревших комментария",
                _ => "устаревших комментариев",
            });
        }
        ReviewMessage::FinishReview => text.literal("Завершить ревью"),
        ReviewMessage::Truncated => text.literal("Это изменение очень большое, поэтому некоторые различия не показаны. Клонируйте репозиторий, чтобы увидеть изменение полностью."),
        ReviewMessage::FileToggleHint => text.literal("Нажмите, чтобы свернуть или развернуть | Alt+нажатие — для всех файлов"),
        ReviewMessage::ViewFile => text.literal("Открыть файл"),
        ReviewMessage::CommentOnLine { line } => {
            text.literal("Комментарий к строке ");
            text.number(line);
        }
        ReviewMessage::StartReview => text.literal("Начать ревью"),
        ReviewMessage::AddSingleComment => text.literal("Добавить отдельный комментарий"),
        ReviewMessage::BinaryFile => text.literal("Двоичный файл не показан."),
        ReviewMessage::TooLarge => text.literal("Различия слишком велики для отображения."),
        ReviewMessage::SubmoduleCommit => text.literal("Коммит подмодуля"),
        ReviewMessage::NoContentChanges => text.literal("Содержимое не изменилось."),
        ReviewMessage::NoChangesTitle => text.literal("Изменений нет"),
        ReviewMessage::NoChangesText => text.literal("Исходная ветка ничего не добавляет к базовой."),
        ReviewMessage::Added => text.literal("Добавлен"),
        ReviewMessage::Deleted => text.literal("Удалён"),
        ReviewMessage::Modified => text.literal("Изменён"),
        ReviewMessage::Renamed => text.literal("Переименован"),
        ReviewMessage::Copied => text.literal("Скопирован"),
        ReviewMessage::Outdated => text.literal("Устарел"),
        ReviewMessage::Pending => text.literal("Ожидает отправки"),
        ReviewMessage::ResolvedThread { path, line } => {
            text.literal("Решённое обсуждение · ");
            text.argument(path);
            text.literal(" ");
            text.argument(line);
        }
        ReviewMessage::ReplyPlaceholder => text.literal("Ответить…"),
        ReviewMessage::AddToReview => text.literal("Добавить в ревью"),
        ReviewMessage::ReopenConversation => text.literal("Открыть обсуждение повторно"),
        ReviewMessage::ResolveConversation => text.literal("Отметить обсуждение решённым"),
        ReviewMessage::ReviewChanges => text.literal("Ревью изменений"),
        ReviewMessage::PendingCount { count } => {
            text.literal("Ожидают отправки: ");
            text.number(count);
        }
        ReviewMessage::ReviewPlaceholder => text.literal("Оставьте комментарий (поддерживается Markdown)"),
        ReviewMessage::RequestChanges => text.literal("Запросить изменения"),
        ReviewMessage::Approve => text.literal("Одобрить"),
        ReviewMessage::DiscardReview => text.literal("Отменить неотправленное ревью"),
        ReviewMessage::DismissReview => text.literal("Отклонить ревью"),
        ReviewMessage::DismissReason => text.literal("Почему это ревью отклоняется?"),
        ReviewMessage::Dismiss => text.literal("Отклонить"),
        ReviewMessage::DismissedBy { by, message } => {
            text.literal("Отклонено пользователем ");
            text.argument(by);
            text.literal(": ");
            text.argument(message);
        }
        ReviewMessage::Reviewers => text.literal("Рецензенты"),
        ReviewMessage::NoReviews => text.literal("Ревью пока нет"),
        ReviewMessage::RemoveRequestFor { name } => {
            text.literal("Отменить запрос ревью у ");
            text.argument(name);
        }
        ReviewMessage::RemoveRequest => text.literal("Отменить запрос"),
        ReviewMessage::RequestReviewFrom => text.literal("Запросить ревью у"),
        ReviewMessage::Username => text.literal("имя пользователя"),
        ReviewMessage::RequestReview => text.literal("Запросить ревью"),
        ReviewMessage::Approved => text.literal("Одобрено"),
        ReviewMessage::ChangesRequested => text.literal("Запрошены изменения"),
        ReviewMessage::Dismissed => text.literal("Отклонено"),
        ReviewMessage::Commented => text.literal("Оставлен комментарий"),
        ReviewMessage::AwaitingReview => text.literal("Ожидает ревью"),
    }
}
