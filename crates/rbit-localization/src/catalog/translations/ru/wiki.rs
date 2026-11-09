//! Russian (`ru`) entries for `catalog/wiki.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::wiki::WikiMessage;
use crate::plural::Plural;

entries! { Ru:
    WikiMessage<'a> => MessageWriter |text| {
        WikiMessage::Tab => text.literal("Вики"),
        WikiMessage::Pages => text.literal("Страницы"),
        WikiMessage::AllPages => text.literal("Все страницы"),
        WikiMessage::PagesLabel => text.literal("Страницы вики"),
        WikiMessage::BreadcrumbLabel => text.literal("Навигационная цепочка"),
        WikiMessage::SearchPages => text.literal("Поиск страниц"),
        WikiMessage::Search => text.literal("Поиск"),
        WikiMessage::NewPage => text.literal("Новая страница"),
        WikiMessage::EditPage => text.literal("Редактировать страницу"),
        WikiMessage::Edit => text.literal("Изменить"),
        WikiMessage::History => text.literal("История"),
        WikiMessage::Delete => text.literal("Удалить"),
        WikiMessage::LastEdited => text.literal("Последнее изменение"),
        WikiMessage::Revision { generation } => {
            text.literal("Ревизия ");
            text.number(u64::try_from(generation).unwrap_or_default());
        }
        WikiMessage::RevisionColumn => text.literal("Ревизия"),
        WikiMessage::Author => text.literal("Автор"),
        WikiMessage::Date => text.literal("Дата"),
        WikiMessage::UnknownAuthor => text.literal("Неизвестный автор"),
        WikiMessage::Created => text.literal("Создана"),
        WikiMessage::Edited => text.literal("Изменена"),
        WikiMessage::Renamed => text.literal("Переименована"),
        WikiMessage::Deleted => text.literal("Удалена"),
        WikiMessage::Restored => text.literal("Восстановлена"),
        WikiMessage::View => text.literal("Открыть"),
        WikiMessage::CompareWithPrevious => text.literal("Сравнить с предыдущей"),
        WikiMessage::Restore => text.literal("Восстановить эту ревизию"),
        WikiMessage::OlderRevisions => text.literal("Более старые ревизии"),
        WikiMessage::OldRevisionNotice => text.literal("Вы просматриваете старую ревизию этой страницы."),
        WikiMessage::ViewCurrent => text.literal("Открыть текущую версию страницы"),
        WikiMessage::HistoryOf { title } => {
            text.literal("История: ");
            text.argument(title);
        }
        WikiMessage::Comparing { base, head } => {
            text.literal("Изменения с ревизии ");
            text.number(u64::try_from(base).unwrap_or_default());
            text.literal(" по ревизию ");
            text.number(u64::try_from(head).unwrap_or_default());
        }
        WikiMessage::Changes { additions, deletions } => {
            text.number(additions);
            text.literal(" ");
            text.literal(match text.plural(additions) {
                Plural::One => "добавление",
                Plural::Few => "добавления",
                _ => "добавлений",
            });
            text.literal(", ");
            text.number(deletions);
            text.literal(" ");
            text.literal(match text.plural(deletions) {
                Plural::One => "удаление",
                Plural::Few => "удаления",
                _ => "удалений",
            });
        }
        WikiMessage::NoDifferences => text.literal("У этих ревизий одинаковое содержимое."),
        WikiMessage::Title => text.literal("Заголовок"),
        WikiMessage::Path => text.literal("Путь"),
        WikiMessage::PathHint => text.literal("Буквы, цифры, - _ и ., разделённые /. Оставьте пустым, чтобы путь был получен из заголовка."),
        WikiMessage::Content => text.literal("Содержимое"),
        WikiMessage::Markdown => text.literal("Markdown"),
        WikiMessage::Save => text.literal("Сохранить страницу"),
        WikiMessage::Preview => text.literal("Предпросмотр"),
        WikiMessage::NothingToPreview => text.literal("Нечего показать."),
        WikiMessage::Cancel => text.literal("Отмена"),
        WikiMessage::TitleEmpty => text.literal("Введите заголовок."),
        WikiMessage::TitleTooLong => text.literal("Заголовок слишком длинный."),
        WikiMessage::TitleInvalid => text.literal("Заголовок содержит недопустимые символы."),
        WikiMessage::PathEmpty => text.literal("Введите путь или заголовок, из которого его можно получить."),
        WikiMessage::PathTooLong => text.literal("Путь или одна из его частей слишком длинные."),
        WikiMessage::PathTooDeep => text.literal("В пути слишком много частей."),
        WikiMessage::PathInvalid => text.literal("Каждая часть пути должна начинаться с буквы или цифры и содержать только буквы, цифры, - _ и ."),
        WikiMessage::PathReserved => text.literal("В пути используется зарезервированное имя."),
        WikiMessage::ContentTooLarge => text.literal("Содержимое слишком большое."),
        WikiMessage::ContentInvalid => text.literal("Содержимое содержит недопустимые символы."),
        WikiMessage::Conflict => text.literal("Эта страница изменилась после того, как вы начали редактирование. Ваш текст сохранён ниже; просмотрите текущую версию страницы, прежде чем сохранять снова."),
        WikiMessage::CompareChanges => text.literal("Сравнить изменения"),
        WikiMessage::PathTaken => text.literal("Этот путь уже занят другой страницей."),
        WikiMessage::Unchanged => text.literal("Ничего не изменилось."),
        WikiMessage::DeleteTitle { title } => {
            text.literal("Удалить «");
            text.argument(title);
            text.literal("»?");
        }
        WikiMessage::DeletePage => text.literal("Удалить страницу"),
        WikiMessage::Disabled => text.literal("Вики для этого репозитория отключена."),
        WikiMessage::About => text.literal("Вики содержит страницы документации со своей историей, отдельно от кода."),
        WikiMessage::TurnOn => text.literal("Включить вики"),
        WikiMessage::TurnOff => text.literal("Отключить вики"),
        WikiMessage::Empty => text.literal("В этой вики пока нет страниц."),
        WikiMessage::CreateFirst => text.literal("Создать первую страницу"),
        WikiMessage::Missing => text.literal("Страницы с таким путём нет."),
        WikiMessage::CreatePath { path } => {
            text.literal("Создать ");
            text.argument(path);
        }
        WikiMessage::SearchCount { count, query } => {
            text.literal("Страниц по запросу «");
            text.argument(query);
            text.literal("»: ");
            text.number(count);
        }
        WikiMessage::NoResults { query } => {
            text.literal("Нет страниц по запросу «");
            text.argument(query);
            text.literal("».");
        }
        WikiMessage::MorePages => text.literal("Другие страницы"),
        WikiMessage::DeleteKeepsHistory => text.literal("Страница будет убрана из вики. Её история сохранится, и пользователи с правом записи смогут восстановить её из удалённых страниц."),
        WikiMessage::DeletedPages => text.literal("Удалённые страницы"),
        WikiMessage::NoDeletedPages => text.literal("Удалённых страниц нет."),
        WikiMessage::DeletedNotice => text.literal("Эта страница удалена. Её история сохранена."),
        WikiMessage::RestorePage => text.literal("Восстановить страницу"),
        WikiMessage::MoreDeletedPages => text.literal("Более ранние удаления"),
        WikiMessage::RestoreBlocked => text.literal("Этот путь теперь занят другой страницей. Сначала переименуйте ту страницу, затем восстановите эту."),
        WikiMessage::RenamedFrom { path } => {
            text.literal("Прежний путь: ");
            text.argument(path);
        }
    }
}
