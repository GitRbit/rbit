//! Russian (`ru`) entries for `catalog/repo/history.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::history::{RepoHistoryLabel, RepoHistoryMessage};
use crate::plural::Plural;

entries! { Ru:
    RepoHistoryLabel => MessageWriter |text| {
        RepoHistoryLabel::BrowseFilesAtCommit => text.literal("Просмотреть файлы на момент этого коммита"),
        RepoHistoryLabel::BrowseFiles => text.literal("Просмотреть файлы"),
        RepoHistoryLabel::Pagination => text.literal("Нумерация страниц"),
        RepoHistoryLabel::Newer => text.literal("Более новые"),
        RepoHistoryLabel::Older => text.literal("Более старые"),
        RepoHistoryLabel::Committed => text.literal("— коммит"),
        RepoHistoryLabel::CommittedBy => text.literal("коммитер"),
        RepoHistoryLabel::Commit => text.literal("коммит"),
        RepoHistoryLabel::Checks => text.literal("Проверки"),
        RepoHistoryLabel::ViewFile => text.literal("Открыть файл"),
        RepoHistoryLabel::PathInThatCommit => text.literal("Путь в том коммите"),
        RepoHistoryLabel::FileEmpty => text.literal("Этот файл пуст."),
        RepoHistoryLabel::BinaryCannotBeBlamed => text.literal("Для двоичных файлов авторство не показывается."),
        RepoHistoryLabel::TooLargeToBlame => text.literal("Файл слишком большой для показа авторства."),
        RepoHistoryLabel::ViewTheFile => text.literal("Открыть файл"),
        RepoHistoryLabel::CompareChanges => text.literal("Сравнение изменений"),
        RepoHistoryLabel::CompareLead => text.literal("Выберите базу и ветку, тег или коммит для сравнения с ней, в том числе между форками."),
        RepoHistoryLabel::Comparison => text.literal("Сравнение"),
        RepoHistoryLabel::MergeBase => text.literal("база слияния"),
        RepoHistoryLabel::Swap => text.literal("Поменять местами"),
        RepoHistoryLabel::CreatePullRequest => text.literal("Создать запрос на слияние"),
        RepoHistoryLabel::UnrelatedHistories => text.literal("У этих ревизий совершенно разные истории, поэтому показать различия невозможно."),
        RepoHistoryLabel::NothingToCompare => text.literal("Нечего сравнивать"),
        RepoHistoryLabel::NothingToCompareText => text.literal("Всё содержимое сравниваемой ревизии уже есть в базе."),
        RepoHistoryLabel::Commits => text.literal("Коммиты"),
        RepoHistoryLabel::Compare => text.literal("Сравнить"),
        RepoHistoryLabel::CompareWithDefault => text.literal("Сравнить с веткой по умолчанию"),
        RepoHistoryLabel::BranchView => text.literal("Вид веток"),
        RepoHistoryLabel::List => text.literal("Список"),
        RepoHistoryLabel::GitTree => text.literal("Дерево Git"),
        RepoHistoryLabel::NoMatchingRefs => text.literal("Подходящих ссылок нет."),
        RepoHistoryLabel::NoBranchesYet => text.literal("Веток пока нет"),
        RepoHistoryLabel::NoBranchesText => text.literal("Отправьте коммит, чтобы создать первую ветку."),
        RepoHistoryLabel::NoTagsYet => text.literal("Тегов пока нет"),
        RepoHistoryLabel::NoTagsText => text.literal("Теги отмечают релизы и другие важные моменты истории."),
        RepoHistoryLabel::Protected => text.literal("Защищена"),
        RepoHistoryLabel::ProtectedTitle => text.literal("К этой ветке применяется правило защиты"),
        RepoHistoryLabel::DownloadTarGz => text.literal("Скачать tar.gz"),
        RepoHistoryLabel::BranchCommitGraph => text.literal("Граф коммитов веток"),
        RepoHistoryLabel::GraphHelp => text.literal("Следуйте по цветным линиям, чтобы проследить историю. Выберите дорожку, чтобы выделить её коммиты."),
        RepoHistoryLabel::ClearHighlight => text.literal("Снять выделение"),
        RepoHistoryLabel::HistoryConnections => text.literal("Связи истории"),
        RepoHistoryLabel::CommitDetails => text.literal("Сведения о коммите"),
        RepoHistoryLabel::Merge => text.literal("Слияние"),
        RepoHistoryLabel::GraphTruncated => text.literal("Показаны последние 200 коммитов всех веток. Линии внизу продолжаются в более старую историю. Откройте историю ветки, чтобы посмотреть дальше."),
        RepoHistoryLabel::LinesAdded => text.literal("Добавлено"),
        RepoHistoryLabel::LinesDeleted => text.literal("Удалено"),
        RepoHistoryLabel::StatusAdded => text.literal("Добавлен"),
        RepoHistoryLabel::StatusDeleted => text.literal("Удалён"),
        RepoHistoryLabel::StatusModified => text.literal("Изменён"),
        RepoHistoryLabel::StatusRenamed => text.literal("Переименован"),
        RepoHistoryLabel::StatusCopied => text.literal("Скопирован"),
        RepoHistoryLabel::ChangedFiles => text.literal("Изменённые файлы"),
        RepoHistoryLabel::FileTree => text.literal("Дерево файлов"),
        RepoHistoryLabel::FilterChangedFiles => text.literal("Фильтр изменённых файлов"),
        RepoHistoryLabel::FilterFiles => text.literal("Фильтр файлов…"),
        RepoHistoryLabel::NoFilesMatch => text.literal("Подходящих файлов нет."),
        RepoHistoryLabel::Changes => text.literal("Изменения"),
        RepoHistoryLabel::ToggleFileTree => text.literal("Показать или скрыть дерево файлов"),
        RepoHistoryLabel::CollapseAll => text.literal("Свернуть все"),
        RepoHistoryLabel::ExpandAll => text.literal("Развернуть все"),
        RepoHistoryLabel::DiffTruncated => text.literal("Это изменение очень большое, поэтому некоторые различия не показаны. Клонируйте репозиторий, чтобы увидеть изменение полностью."),
        RepoHistoryLabel::CollapseHint => text.literal("Нажмите, чтобы свернуть или развернуть · Alt+нажатие — для всех файлов"),
        RepoHistoryLabel::CopyPath => text.literal("Скопировать путь"),
        RepoHistoryLabel::ViewFileAtCommit => text.literal("Открыть файл на момент этого коммита"),
        RepoHistoryLabel::View => text.literal("Открыть"),
        RepoHistoryLabel::BinaryNotShown => text.literal("Двоичный файл не показан."),
        RepoHistoryLabel::DiffTooLarge => text.literal("Различия слишком велики для отображения."),
        RepoHistoryLabel::SubmoduleCommit => text.literal("Коммит подмодуля"),
        RepoHistoryLabel::NoContentChanges => text.literal("Содержимое не изменилось."),
    }
    RepoHistoryMessage<'a> => MessageWriter |text| {
        RepoHistoryMessage::CommitsOn { date } => {
            text.literal("Коммиты за ");
            text.argument(date);
        }
        RepoHistoryMessage::ViewCommit { id } => {
            text.literal("Открыть коммит ");
            text.argument(id);
        }
        RepoHistoryMessage::ParentNoun { count } => {
            text.literal(match text.plural(count as u64) {
                Plural::One => "родитель",
                Plural::Few => "родителя",
                _ => "родителей",
            });
        }
        RepoHistoryMessage::BlameOf { file } => {
            text.literal("Авторство: ");
            text.argument(file);
        }
        RepoHistoryMessage::BlameTitle { path } => {
            text.literal("Авторство: ");
            text.argument(path);
        }
        RepoHistoryMessage::Ahead { count } => {
            text.argument(count);
            text.literal(" впереди");
        }
        RepoHistoryMessage::Behind { count } => {
            text.argument(count);
            text.literal(" позади");
        }
        RepoHistoryMessage::OnlyNewest { count } => {
            text.literal("Показаны только ");
            text.number(count as u64);
            text.literal(" ");
            text.literal(match text.plural(count as u64) {
                Plural::One => "последний коммит",
                Plural::Few => "последних коммита",
                _ => "последних коммитов",
            });
            text.literal(".");
        }
        RepoHistoryMessage::HighlightTrack { track } => {
            text.literal("Выделить дорожку ");
            text.number(track as u64);
        }
        RepoHistoryMessage::Track { track } => {
            text.literal("Дорожка ");
            text.number(track as u64);
        }
        RepoHistoryMessage::BranchName { name } => {
            text.literal("Ветка: ");
            text.argument(name);
        }
        RepoHistoryMessage::MoreBranches { count } => {
            text.literal("ещё ");
            text.number(count as u64);
            text.literal(" ");
            text.literal(match text.plural(count as u64) {
                Plural::One => "ветка",
                Plural::Few => "ветки",
                _ => "веток",
            });
        }
        RepoHistoryMessage::FilesChangedNoun { count } => {
            text.literal(match text.plural(count as u64) {
                Plural::One => "файл изменён",
                Plural::Few => "файла изменено",
                _ => "файлов изменено",
            });
        }
        RepoHistoryMessage::CompareTitle { base, head } => {
            text.literal("Сравнение ");
            text.argument(base);
            text.literal("...");
            text.argument(head);
        }
    }
}
