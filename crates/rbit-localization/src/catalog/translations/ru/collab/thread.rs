//! Russian (`ru`) entries for `catalog/collab/thread.rs`.

use crate::catalog::collab::thread::ThreadMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ru:
    ThreadMessage => MessageWriter |text| {
        ThreadMessage::Open => text.literal("Открыто"),
        ThreadMessage::Closed => text.literal("Закрыто"),
        ThreadMessage::Merged => text.literal("Слито"),
        ThreadMessage::Draft => text.literal("Черновик"),
        ThreadMessage::Title => text.literal("Заголовок"),
        ThreadMessage::Description => text.literal("Описание"),
        ThreadMessage::Edit => text.literal("Изменить"),
        ThreadMessage::Save => text.literal("Сохранить"),
        ThreadMessage::Cancel => text.literal("Отмена"),
        ThreadMessage::Delete => text.literal("Удалить"),
        ThreadMessage::Optional => text.literal("необязательно"),
        ThreadMessage::MarkdownSupported => text.literal("Поддерживается Markdown"),
        ThreadMessage::SignIn => text.literal("Войдите"),
        ThreadMessage::LoadMore => text.literal("Загрузить ещё"),
        ThreadMessage::AddComment => text.literal("Добавить комментарий"),
        ThreadMessage::CommentPlaceholder => text.literal("Напишите комментарий. Используйте @ для пользователей и # для задач или запросов на слияние."),
        ThreadMessage::MentionSuggestions => text.literal("Варианты упоминаний"),
        ThreadMessage::CommentHint => text.literal("Поддерживается Markdown. Используйте @name, #123 или owner/repo#123, чтобы сослаться на пользователя, задачу или запрос на слияние."),
        ThreadMessage::LockedHint => text.literal("Это обсуждение заблокировано; комментировать могут только соавторы."),
        ThreadMessage::CloseIssue => text.literal("Закрыть задачу"),
        ThreadMessage::ClosePull => text.literal("Закрыть запрос на слияние"),
        ThreadMessage::CloseNotPlanned => text.literal("Закрыть как незапланированное"),
        ThreadMessage::ReopenIssue => text.literal("Открыть задачу повторно"),
        ThreadMessage::ReopenPull => text.literal("Открыть запрос на слияние повторно"),
        ThreadMessage::Comment => text.literal("Комментировать"),
        ThreadMessage::LockConversation => text.literal("Заблокировать обсуждение"),
        ThreadMessage::UnlockConversation => text.literal("Разблокировать обсуждение"),
        ThreadMessage::Bot => text.literal("бот"),
        ThreadMessage::Edited => text.literal("изменено"),
        ThreadMessage::Author => text.literal("Автор"),
        ThreadMessage::Reply => text.literal("Ответить"),
        ThreadMessage::DeleteCommentConfirm => text.literal("Удалить этот комментарий?"),
        ThreadMessage::DeleteComment => text.literal("Удалить комментарий"),
        ThreadMessage::NoDescription => text.literal("Описание не указано."),
    }
}
