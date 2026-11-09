//! Russian (`ru`) entries for `catalog/repo/edit.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::edit::{RepoEditLabel, RepoEditMessage};
use crate::plural::Plural;

entries! { Ru:
    RepoEditLabel => MessageWriter |text| {
        RepoEditLabel::NewFile => text.literal("Новый файл"),
        RepoEditLabel::Edit => text.literal("Редактирование"),
        RepoEditLabel::On => text.literal("В ветке"),
        RepoEditLabel::NewFileHint => text.literal("Укажите путь с /, чтобы создать каталоги."),
        RepoEditLabel::EditHint => text.literal("Измените путь, чтобы переименовать или переместить файл."),
        RepoEditLabel::BackToFile => text.literal("Вернуться к файлу"),
        RepoEditLabel::Path => text.literal("Путь"),
        RepoEditLabel::Content => text.literal("Содержимое"),
        RepoEditLabel::CommitChanges => text.literal("Зафиксировать изменения"),
        RepoEditLabel::Cancel => text.literal("Отмена"),
        RepoEditLabel::Delete => text.literal("Удаление"),
        RepoEditLabel::DeleteHint => text.literal("Файл останется в истории; каталог, который станет пустым, исчезнет."),
        RepoEditLabel::DeleteFile => text.literal("Удалить файл"),
        RepoEditLabel::UploadFiles => text.literal("Загрузить файлы"),
        RepoEditLabel::Into => text.literal("В каталог"),
        RepoEditLabel::OnBranch => text.literal("в ветке"),
        RepoEditLabel::UploadHint => text.literal("Файлы с такими же именами будут заменены."),
        RepoEditLabel::Files => text.literal("Файлы"),
        RepoEditLabel::NothingCommitted => text.literal("Ничего не зафиксировано."),
        RepoEditLabel::ReviewNewCommits => text.literal("Просмотрите новые коммиты"),
        RepoEditLabel::KeptAndCommittedOnTop => text.literal("и зафиксируйте изменения снова: они сохранены ниже и будут добавлены поверх новой версии."),
        RepoEditLabel::BranchGone => text.literal("Ничего не зафиксировано. Зафиксируйте изменения в новой ветке; они сохранены ниже."),
        RepoEditLabel::CommitChangesLegend => text.literal("Фиксация изменений"),
        RepoEditLabel::CommitMessage => text.literal("Сообщение коммита"),
        RepoEditLabel::ExtendedDescription => text.literal("Подробное описание"),
        RepoEditLabel::Optional => text.literal("необязательно"),
        RepoEditLabel::CommitDirectlyTo => text.literal("Зафиксировать напрямую в"),
        RepoEditLabel::NotAllowed => text.literal("Недоступно:"),
        RepoEditLabel::NewBranchAndPull => text.literal("Создать новую ветку и открыть запрос на слияние"),
        RepoEditLabel::NewBranchName => text.literal("Имя новой ветки"),
    }
    RepoEditMessage<'a> => MessageWriter |text| {
        RepoEditMessage::AtMost { size } => {
            text.literal("Не более ");
            text.argument(size);
        }
        RepoEditMessage::UploadLimits { files, size } => {
            text.literal("До ");
            text.number(files as u64);
            text.literal(" ");
            text.literal(match text.plural(files as u64) {
                Plural::One => "файла",
                _ => "файлов",
            });
            text.literal(" размером не более ");
            text.argument(size);
            text.literal(" каждый, в одном коммите.");
        }
        RepoEditMessage::BranchChanged { branch } => {
            text.literal("Ветка ");
            text.argument(branch);
            text.literal(" изменилась с момента начала редактирования");
        }
        RepoEditMessage::BranchMissing { branch } => {
            text.literal("Ветки ");
            text.argument(branch);
            text.literal(" больше не существует");
        }
        RepoEditMessage::NowPointsAt => text.literal("Теперь она указывает на"),
        RepoEditMessage::NotEditable { limit } => {
            text.literal("Этот файл двоичный или больше ");
            text.argument(limit);
            text.literal(", поэтому его нельзя редактировать в браузере. Его можно удалить или загрузить замену.");
        }
        RepoEditMessage::EditingTitle { path } => {
            text.literal("Редактирование: ");
            text.argument(path);
        }
        RepoEditMessage::DeleteTitle { path } => {
            text.literal("Удаление: ");
            text.argument(path);
        }
    }
}
