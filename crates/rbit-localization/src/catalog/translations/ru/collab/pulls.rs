//! Russian (`ru`) entries for `catalog/collab/pulls.rs`.

use crate::catalog::collab::pulls::PullMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Ru:
    PullMessage<'a> => MessageWriter |text| {
        PullMessage::PullRequests => text.literal("Запросы на слияние"),
        PullMessage::SearchPulls => text.literal("Поиск запросов на слияние"),
        PullMessage::ClearSearch => text.literal("Сбросить поиск и фильтры"),
        PullMessage::FiltersLabel => text.literal("Фильтры запросов на слияние"),
        PullMessage::StateNav => text.literal("Состояние запроса на слияние"),
        PullMessage::AuthorFilter { author } => {
            text.literal("автор: ");
            text.argument(author);
        }
        PullMessage::New => text.literal("Новый"),
        PullMessage::NewSuffix => text.literal("\u{a0}запрос на слияние"),
        PullMessage::EmptyTitle => text.literal("Запросов на слияние здесь нет"),
        PullMessage::EmptyText => text.literal("Запросы на слияние предлагают изменения из ветки или форка. Сравните ветки, чтобы открыть запрос."),
        PullMessage::OpenTitle => text.literal("Открыть запрос на слияние"),
        PullMessage::OpenIntro => text.literal("Предложите изменения из ветки этого репозитория или форка для слияния в базовую ветку."),
        PullMessage::OpenAsDraft => text.literal("Открыть как черновик"),
        PullMessage::Create => text.literal("Создать запрос на слияние"),
        PullMessage::DifferentHistories => text.literal("У этих веток совершенно разные истории."),
        PullMessage::ChooseBranches => text.literal("Выберите две существующие ветки для сравнения."),
        PullMessage::UpToDate { base, head } => {
            text.argument(base);
            text.literal(" уже содержит все коммиты из ");
            text.argument(head);
            text.literal(".");
        }
        PullMessage::BaseRepository => text.literal("Базовый репозиторий"),
        PullMessage::BaseRefs => text.literal("Базовые ветки и теги"),
        PullMessage::Base => text.literal("База"),
        PullMessage::HeadRepository => text.literal("Исходный репозиторий"),
        PullMessage::Compare => text.literal("Сравнить"),
        PullMessage::CompareRefs => text.literal("Сравнить ветки и теги"),
        PullMessage::PullRequest => text.literal("Запрос на слияние"),
        PullMessage::Conversation => text.literal("Обсуждение"),
        PullMessage::Commits => text.literal("Коммиты"),
        PullMessage::FilesChanged => text.literal("Изменённые файлы"),
        PullMessage::CommitsPhrase { count: Some(count) } => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "коммит",
                Plural::Few => "коммита",
                _ => "коммитов",
            });
        }
        PullMessage::CommitsPhrase { count: None } => text.literal("коммиты"),
        PullMessage::ChangeBase => text.literal("Изменить базовую ветку"),
        PullMessage::BaseBranch => text.literal("Базовая ветка"),
        PullMessage::BaseBranches => text.literal("Базовые ветки"),
        PullMessage::ChangeBaseHint => text.literal("Смена базы обновляет коммиты, различия и проверки слияния."),
        PullMessage::UpdateBase => text.literal("Обновить базовую ветку"),
        PullMessage::IsClosed => text.literal("Этот запрос на слияние закрыт."),
        PullMessage::IsMerged => text.literal("Запрос на слияние слит."),
        PullMessage::DeleteBranch { branch } => {
            text.literal("Удалить ");
            text.argument(branch);
        }
        PullMessage::ConvertToDraft => text.literal("Перевести в черновик"),
        PullMessage::NoCommitsTitle => text.literal("Коммитов нет"),
        PullMessage::NoCommitsText => text.literal("Исходная и базовая ветки указывают на один и тот же коммит."),
        PullMessage::NewestCommits { count } => {
            text.literal("Показаны только ");
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "последний коммит",
                Plural::Few => "последних коммита",
                _ => "последних коммитов",
            });
            text.literal(".");
        }
    }
}
