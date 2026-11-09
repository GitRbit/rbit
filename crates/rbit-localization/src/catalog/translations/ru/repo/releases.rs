//! Russian (`ru`) entries for `catalog/repo/releases.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::releases::{ReleaseLabel, ReleaseMessage};
use crate::plural::Plural;

entries! { Ru:
    ReleaseLabel => MessageWriter |text| {
        ReleaseLabel::Releases => text.literal("Релизы"),
        ReleaseLabel::NewRelease => text.literal("Новый релиз"),
        ReleaseLabel::EditRelease => text.literal("Редактирование релиза"),
        ReleaseLabel::NoReleasesYet => text.literal("Релизов пока нет"),
        ReleaseLabel::EmptyForWriters => text.literal("Релиз объединяет тег с описанием и файлами. Отправьте тег, а затем создайте для него релиз."),
        ReleaseLabel::EmptyForReaders => text.literal("Релизы отмечают версии этого проекта с описанием и файлами для скачивания."),
        ReleaseLabel::Draft => text.literal("Черновик"),
        ReleaseLabel::PreRelease => text.literal("Предварительный релиз"),
        ReleaseLabel::Latest => text.literal("Последний"),
        ReleaseLabel::Bot => text.literal("бот"),
        ReleaseLabel::Created => text.literal("создан"),
        ReleaseLabel::Edit => text.literal("Изменить"),
        ReleaseLabel::Downloads => text.literal("Файлы для скачивания"),
        ReleaseLabel::SourceZip => text.literal("Исходный код (zip)"),
        ReleaseLabel::SourceTarGz => text.literal("Исходный код (tar.gz)"),
        ReleaseLabel::Files => text.literal("Файлы"),
        ReleaseLabel::Remove => text.literal("Удалить"),
        ReleaseLabel::FileToAttach => text.literal("Прикрепляемый файл"),
        ReleaseLabel::Upload => text.literal("Загрузить"),
        ReleaseLabel::DeleteTitle => text.literal("Удалить этот релиз"),
        ReleaseLabel::DeleteHintStart => text.literal("Релиз, его описание и файлы будут удалены. Тег"),
        ReleaseLabel::DeleteHintEnd => text.literal("останется."),
        ReleaseLabel::DeleteRelease => text.literal("Удалить релиз"),
        ReleaseLabel::FormLead => text.literal("Релиз описывает тег: его описание и файлы для скачивания."),
        ReleaseLabel::Tag => text.literal("Тег"),
        ReleaseLabel::AnExisting => text.literal("Существующий "),
        ReleaseLabel::TagLink => text.literal("тег"),
        ReleaseLabel::OrNewTag => text.literal(" или новый, созданный из ветки ниже."),
        ReleaseLabel::CreateTagFrom => text.literal("Создать новый тег из"),
        ReleaseLabel::TargetHint => text.literal("используется, только если тега ещё нет"),
        ReleaseLabel::OneOfTags => text.literal("Один из существующих "),
        ReleaseLabel::TagsEnd => text.literal("."),
        ReleaseLabel::TagsLink => text.literal("тегов"),
        ReleaseLabel::Title => text.literal("Заголовок"),
        ReleaseLabel::TitleHint => text.literal("необязательно, по умолчанию — имя тега"),
        ReleaseLabel::Notes => text.literal("Описание"),
        ReleaseLabel::IsPreRelease => text.literal("Это предварительный релиз"),
        ReleaseLabel::PreReleaseHint => text.literal("Отмечен как не готовый к продакшену; последним предпочтительно считается полноценный релиз."),
        ReleaseLabel::SaveRelease => text.literal("Сохранить релиз"),
        ReleaseLabel::PublishRelease => text.literal("Опубликовать релиз"),
        ReleaseLabel::SaveDraft => text.literal("Сохранить черновик"),
        ReleaseLabel::Cancel => text.literal("Отмена"),
    }
    ReleaseMessage<'a> => MessageWriter |text| {
        ReleaseMessage::Checksum { sha256 } => {
            text.literal("SHA-256 ");
            text.argument(sha256);
        }
        ReleaseMessage::Downloads { count } => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "скачивание",
                Plural::Few => "скачивания",
                _ => "скачиваний",
            });
        }
        ReleaseMessage::RemoveFile { name } => {
            text.literal("Удалить ");
            text.argument(name);
        }
        ReleaseMessage::UploadLimit { size } => {
            text.literal("До ");
            text.argument(size);
            text.literal(" на файл. Файлы всегда отдаются для скачивания и никогда не отображаются.");
        }
        ReleaseMessage::Uploaded { name } => {
            text.literal("Файл ");
            text.argument(name);
            text.literal(" загружен.");
        }
        ReleaseMessage::Removed { name } => {
            text.literal("Файл ");
            text.argument(name);
            text.literal(" удалён.");
        }
        ReleaseMessage::EditTitle { tag } => {
            text.literal("Редактирование: ");
            text.argument(tag);
        }
    }
}
