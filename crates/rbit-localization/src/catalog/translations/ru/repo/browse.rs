//! Russian (`ru`) entries for `catalog/repo/browse.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::browse::RepoBrowseSentence;
use crate::catalog::repo::browse::{RepoBrowseLabel, RepoBrowseMessage};
use crate::plural::Plural;

entries! { Ru:
    RepoBrowseLabel => MessageWriter |text| {
        RepoBrowseLabel::About => text.literal("О репозитории"),
        RepoBrowseLabel::NoDescription => text.literal("Описания нет."),
        RepoBrowseLabel::Visibility => text.literal("Видимость"),
        RepoBrowseLabel::History => text.literal("История"),
        RepoBrowseLabel::CommitHistory => text.literal("История коммитов"),
        RepoBrowseLabel::AddFile => text.literal("Добавить файл"),
        RepoBrowseLabel::UploadFiles => text.literal("Загрузить файлы"),
        RepoBrowseLabel::Files => text.literal("Файлы"),
        RepoBrowseLabel::ParentDirectory => text.literal("Родительский каталог"),
        RepoBrowseLabel::NameColumn => text.literal("Имя"),
        RepoBrowseLabel::LastCommitMessageColumn => text.literal("Сообщение последнего коммита"),
        RepoBrowseLabel::LastCommitDateColumn => text.literal("Дата последнего коммита"),
        RepoBrowseLabel::Path => text.literal("Путь"),
        RepoBrowseLabel::Latest => text.literal("Последний коммит"),
        RepoBrowseLabel::YourAccess => text.literal("Ваш доступ"),
        RepoBrowseLabel::LeaveRepository => text.literal("Покинуть репозиторий"),
        RepoBrowseLabel::Languages => text.literal("Языки"),
        RepoBrowseLabel::RepositoryLinks => text.literal("Ссылки репозитория"),
        RepoBrowseLabel::StoredWithLfs => text.literal("Хранится в Git LFS"),
        RepoBrowseLabel::LfsObjectMissing => text.literal("Объекта нет на этом сервере"),
        RepoBrowseLabel::EditThisFile => text.literal("Редактировать файл"),
        RepoBrowseLabel::DeleteThisFile => text.literal("Удалить файл"),
        RepoBrowseLabel::Blame => text.literal("Авторство"),
        RepoBrowseLabel::Raw => text.literal("Исходник"),
        RepoBrowseLabel::Download => text.literal("Скачать"),
        RepoBrowseLabel::BinaryNotShown => text.literal("Двоичный файл не показан."),
        RepoBrowseLabel::TooLargeToDisplay => text.literal("Файл слишком большой для отображения."),
        RepoBrowseLabel::ViewRaw => text.literal("Открыть исходник"),
        RepoBrowseLabel::RepositoryDocuments => text.literal("Документы репозитория"),
        RepoBrowseLabel::DocumentOutline => text.literal("Структура документа"),
        RepoBrowseLabel::FilterHeadings => text.literal("Фильтр заголовков"),
        RepoBrowseLabel::DocumentHeadings => text.literal("Заголовки документа"),
        RepoBrowseLabel::NoMatchingHeadings => text.literal("Подходящих заголовков нет."),
        RepoBrowseLabel::EmptyTitle => text.literal("Этот репозиторий пуст"),
        RepoBrowseLabel::EmptyLead => text.literal("Отправьте существующий проект или начните новый из командной строки."),
        RepoBrowseLabel::RepositoryUrl => text.literal("URL репозитория"),
        RepoBrowseLabel::CopyRepositoryUrl => text.literal("Скопировать URL репозитория"),
        RepoBrowseLabel::SshRepositoryUrl => text.literal("SSH-URL репозитория"),
        RepoBrowseLabel::CopySshRepositoryUrl => text.literal("Скопировать SSH-URL репозитория"),
        RepoBrowseLabel::CreateNewRepository => text.literal("Создать новый репозиторий"),
        RepoBrowseLabel::PushExistingRepository => text.literal("Отправить существующий репозиторий"),
        RepoBrowseLabel::CloseFileBrowser => text.literal("Закрыть обозреватель файлов"),
        RepoBrowseLabel::LanguageFiles => text.literal("Файлы языка"),
        RepoBrowseLabel::NoFilesForLanguage => text.literal("Файлов на этом языке не найдено."),
        RepoBrowseLabel::NoMatchingFiles => text.literal("Подходящих файлов нет."),
        RepoBrowseLabel::FileContent => text.literal("Содержимое файла"),
        RepoBrowseLabel::SelectFileToPreview => text.literal("Выберите файл, чтобы посмотреть его содержимое."),
        RepoBrowseLabel::SearchThisListing => text.literal("Поиск в этом списке"),
        RepoBrowseLabel::SearchNames => text.literal("Поиск по именам"),
        RepoBrowseLabel::Search => text.literal("Поиск"),
        RepoBrowseLabel::ListingPages => text.literal("Страницы списка"),
        RepoBrowseLabel::Previous => text.literal("Назад"),
        RepoBrowseLabel::Next => text.literal("Далее"),
        RepoBrowseLabel::BranchesAndTags => text.literal("Ветки и теги"),
        RepoBrowseLabel::BrowseBranchesAndTags => text.literal("Просмотр веток и тегов"),
        RepoBrowseLabel::FindBranchOrTag => text.literal("Найти ветку или тег"),
        RepoBrowseLabel::Find => text.literal("Найти"),
        RepoBrowseLabel::Branch => text.literal("Ветка"),
        RepoBrowseLabel::Tag => text.literal("Тег"),
        RepoBrowseLabel::Default => text.literal("по умолчанию"),
        RepoBrowseLabel::NoMatchingRefs => text.literal("Подходящих веток и тегов нет."),
        RepoBrowseLabel::NoForksYet => text.literal("Форков пока нет"),
        RepoBrowseLabel::ForksEmpty => text.literal("Форки — это копии репозитория, с которыми другие работают независимо."),
        RepoBrowseLabel::ForkLead => text.literal("Форк — это ваша собственная копия веток и тегов репозитория. Он сохраняет видимость оригинала."),
        RepoBrowseLabel::ForkLeadPrivate => text.literal("Форк — это ваша собственная копия веток и тегов репозитория. Он сохраняет видимость оригинала — этот форк останется приватным."),
        RepoBrowseLabel::Owner => text.literal("Владелец"),
        RepoBrowseLabel::Name => text.literal("Имя"),
        RepoBrowseLabel::CreateFork => text.literal("Создать форк"),
        RepoBrowseLabel::Cancel => text.literal("Отмена"),
        RepoBrowseLabel::NoStargazersYet => text.literal("Звёзд пока нет"),
        RepoBrowseLabel::StargazersEmpty => text.literal("Здесь появятся люди, поставившие звезду этому репозиторию."),
        RepoBrowseLabel::RepositoryActivity => text.literal("Активность репозитория"),
        RepoBrowseLabel::GoToFile => text.literal("Перейти к файлу"),
        RepoBrowseLabel::ClearFileFilters => text.literal("Сбросить фильтры"),
        RepoBrowseLabel::MatchingFiles => text.literal("Подходящие файлы"),
        RepoBrowseLabel::LoadingFiles => text.literal("Загрузка файлов…"),
        RepoBrowseLabel::UnableToLoadFiles => text.literal("Не удалось загрузить файлы. Нажмите Enter, чтобы выполнить поиск."),
        RepoBrowseLabel::SeeAllMatches => text.literal("Показать все совпадения"),
        RepoBrowseLabel::NoFilesInRevision => text.literal("В этой ревизии нет файлов."),
        RepoBrowseLabel::TooManyFilesToSearch => text.literal("В этой ревизии слишком много файлов, чтобы искать по всем."),
    }
    RepoBrowseMessage<'a> => MessageWriter |text| {
        RepoBrowseMessage::BranchNoun { count } => {
            text.literal(match text.plural(count as u64) {
                Plural::One => "ветка",
                Plural::Few => "ветки",
                _ => "веток",
            });
        }
        RepoBrowseMessage::TagNoun { count } => {
            text.literal(match text.plural(count as u64) {
                Plural::One => "тег",
                Plural::Few => "тега",
                _ => "тегов",
            });
        }
        RepoBrowseMessage::CollaboratorAccess { permission } => {
            text.literal("Права «");
            text.argument(permission);
            text.literal("» в качестве соавтора.");
        }
        RepoBrowseMessage::LineCount { count } => {
            text.number(count as u64);
            text.literal(" ");
            text.literal(match text.plural(count as u64) {
                Plural::One => "строка",
                Plural::Few => "строки",
                _ => "строк",
            });
        }
        RepoBrowseMessage::LanguageFiles { language } => {
            text.literal("Файлы: ");
            text.argument(language);
        }
        RepoBrowseMessage::ListingResults { total, page, pages } => {
            text.number(total as u64);
            text.literal(" ");
            text.literal(match text.plural(total as u64) {
                Plural::One => "результат",
                Plural::Few => "результата",
                _ => "результатов",
            });
            text.literal(" · ");
            text.number(page as u64);
            text.literal(" / ");
            text.number(pages as u64);
        }
        RepoBrowseMessage::RefPage { page, pages, total } => {
            text.number(page as u64);
            text.literal(" / ");
            text.number(pages as u64);
            text.literal(" · ");
            text.number(total as u64);
            text.literal(" ");
            text.literal(match text.plural(total as u64) {
                Plural::One => "ссылка",
                Plural::Few => "ссылки",
                _ => "ссылок",
            });
        }
        RepoBrowseMessage::ForkTitle { owner, name } => {
            text.literal("Форк ");
            text.argument(owner);
            text.literal("/");
            text.argument(name);
        }
        RepoBrowseMessage::CommitNoun { count } => text.literal(match text.plural(count as u64) {
            Plural::One => "коммит",
            Plural::Few => "коммита",
            _ => "коммитов",
        }),
        RepoBrowseMessage::NoFileMatches { query } => {
            text.literal("Нет файлов, подходящих под «");
            text.argument(query);
            text.literal("».");
        }
        RepoBrowseMessage::MoreFileMatches { count } => {
            text.literal("Ещё совпадений: ");
            text.number(count as u64);
            text.literal(". Введите больше символов имени, чтобы сузить список.");
        }
    }
    RepoBrowseSentence => RichWriter |text| {
        RepoBrowseSentence::EmptySshNote => {
            text.literal("Команды ниже используют HTTPS. Чтобы отправлять изменения по SSH, используйте SSH-URL и добавьте ключ в ");
            text.slot(0);
            text.literal(".");
        }
    }
}
