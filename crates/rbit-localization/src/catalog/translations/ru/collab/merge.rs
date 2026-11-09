//! Russian (`ru`) entries for `catalog/collab/merge.rs`.

use crate::catalog::collab::merge::MergeMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Ru:
    MergeMessage<'a> => MessageWriter |text| {
        MergeMessage::Merge => text.literal("Слияние"),
        MergeMessage::ConflictingFiles => text.literal("Конфликтующие файлы"),
        MergeMessage::WorkInProgress => text.literal("Работа над этим запросом на слияние ещё не завершена."),
        MergeMessage::ReadyForReview => text.literal("Готов к ревью"),
        MergeMessage::MergeMethod => text.literal("Способ слияния"),
        MergeMessage::CreateMergeCommit => text.literal("Создать коммит слияния"),
        MergeMessage::SquashAndMerge => text.literal("Объединить коммиты и слить"),
        MergeMessage::RebaseAndMerge => text.literal("Перебазировать и слить"),
        MergeMessage::EditMessage => text.literal("Изменить сообщение коммита"),
        MergeMessage::Message => text.literal("Сообщение"),
        MergeMessage::TitlePlaceholder { title, squash } => {
            text.argument(title);
            text.literal(" — или при объединении коммитов ");
            text.argument(squash);
        }
        MergeMessage::DeleteHead => text.literal("Удалить исходную ветку после слияния"),
        MergeMessage::Bypass => text.literal("Слить без выполнения требований защиты ветки (действие попадёт в журнал аудита)"),
        MergeMessage::MergePull => text.literal("Слить запрос на слияние"),
        MergeMessage::WriteAccessOnly => text.literal("Выполнять слияние могут только пользователи с правом записи в этот репозиторий."),
        MergeMessage::PullOpen => text.literal("Запрос на слияние открыт"),
        MergeMessage::PullClosed => text.literal("Запрос на слияние закрыт"),
        MergeMessage::StillDraft => text.literal("Этот запрос на слияние ещё черновик"),
        MergeMessage::HeadExists => text.literal("Исходная ветка существует"),
        MergeMessage::HeadDeleted => text.literal("Исходная ветка удалена"),
        MergeMessage::NoConflicts => text.literal("Нет конфликтов с базовой веткой"),
        MergeMessage::Conflicts => text.literal("В этой ветке есть конфликты, которые нужно разрешить"),
        MergeMessage::CheckingConflicts => text.literal("Проверка конфликтов…"),
        MergeMessage::MergeabilityError => text.literal("Не удалось проверить возможность слияния"),
        MergeMessage::Approvals { have, need } => {
            text.number(have);
            text.literal(" из ");
            text.number(need);
            text.literal(" ");
            text.literal(match text.plural(need) {
                Plural::One => "обязательного одобрения",
                _ => "обязательных одобрений",
            });
        }
        MergeMessage::NoChangesRequested => text.literal("Запросов изменений нет"),
        MergeMessage::ChangesRequestedBy { count } => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => " рецензент запросил изменения",
                Plural::Few => " рецензента запросили изменения",
                _ => " рецензентов запросили изменения",
            });
        }
        MergeMessage::MayPush => text.literal("Вы можете отправлять изменения в базовую ветку"),
        MergeMessage::MayNotPush => text.literal("Вам не разрешено отправлять изменения в базовую ветку"),
        MergeMessage::NoChecks => text.literal("Нет проверок статуса"),
        MergeMessage::ChecksPending => text.literal("Некоторые проверки ещё не завершены"),
        MergeMessage::ChecksPassed => text.literal("Все проверки пройдены"),
        MergeMessage::ChecksFailed => text.literal("Некоторые проверки не пройдены"),
        MergeMessage::RequiredPassed { count } => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => " обязательная проверка пройдена",
                Plural::Few => " обязательные проверки пройдены",
                _ => " обязательных проверок пройдено",
            });
        }
        MergeMessage::RequiredFailed { failing, required } => {
            text.literal("Не пройдено обязательных проверок: ");
            text.number(failing);
            text.literal(" из ");
            text.number(required);
        }
        MergeMessage::RequiredWaiting { waiting, required } => {
            text.literal("Ожидание обязательных проверок: ");
            text.number(waiting);
            text.literal(" из ");
            text.number(required);
        }
    }
}
