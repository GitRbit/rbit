//! Russian (`ru`) entries for `catalog/explore.rs`.

use crate::catalog::RepositoryVisibility;
use crate::catalog::explore::{Message, signed};
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

/// "5 репозиториев", with the noun agreeing with the count.
fn repositories(text: &mut MessageWriter, count: u64) {
    text.number(count);
    text.literal(match text.plural(count) {
        Plural::One => " репозиторий",
        Plural::Few => " репозитория",
        _ => " репозиториев",
    });
}

entries! { Ru:
    Message<'a> => MessageWriter |text| {
        Message::RepositoriesTitle => text.literal("Репозитории"),
        Message::RepositoryCount { count } => repositories(text, count),
        Message::OwnerRepositories { owner } => {
            text.literal("Репозитории владельца ");
            text.argument(owner);
        }
        Message::Visibility { visibility } => text.literal(match visibility {
            RepositoryVisibility::Public => "Публичный",
            RepositoryVisibility::Private => "Приватный",
            RepositoryVisibility::Internal => "Внутренний",
        }),
        Message::EmptyRepositories => text.literal("Репозитории не найдены."),
        Message::SearchRepositories => text.literal("Поиск репозиториев"),
        Message::SearchLabel => text.literal("Поиск"),
        Message::NextPage => text.literal("Далее"),
        Message::PreviousPage => text.literal("Назад"),
        Message::PageNumber { page } => {
            text.literal("Страница ");
            text.number(page);
        }
        Message::RepositoryLinkLabel { owner, name } => {
            text.literal("Открыть репозиторий ");
            text.argument(owner);
            text.literal("/");
            text.argument(name);
        }
        Message::ExploreTitle => text.literal("Обзор"),
        Message::ExploreSubtitle { site } => {
            text.literal("Публичные репозитории на сервере ");
            text.argument(site);
            text.literal(".");
        }
        Message::ExploreSearchPlaceholder => text.literal("Поиск по имени или описанию"),
        Message::SortBy => text.literal("Сортировка"),
        Message::SortUpdated => text.literal("По обновлению"),
        Message::SortName => text.literal("По имени"),
        Message::SortNewest => text.literal("Сначала новые"),
        Message::ArchiveFilterLabel => text.literal("Архивные репозитории"),
        Message::ArchiveAll => text.literal("Все"),
        Message::ArchiveActive => text.literal("Активные"),
        Message::Archived => text.literal("Архивирован"),
        Message::NothingHereYet => text.literal("Здесь пока ничего нет"),
        Message::PublicRepositoriesEmpty => text.literal("Публичные репозитории появятся здесь, когда они будут созданы."),
        Message::NoMatchingRepositories => text.literal("Подходящих репозиториев нет"),
        Message::SearchSuggestion => text.literal("Попробуйте более короткий запрос или другие слова."),
        Message::CountForQuery { count, query } => {
            repositories(text, count);
            if !query.is_empty() {
                text.literal(" по запросу «");
                text.argument(query);
                text.literal("»");
            }
        }
        Message::PaginationLabel => text.literal("Постраничная навигация"),
        Message::PageOf { page, pages } => {
            text.literal("Страница ");
            text.number(page.into());
            text.literal(" из ");
            text.number(pages.into());
        }
        Message::Stars { count } => {
            signed(text, count);
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => " звезда",
                Plural::Few => " звезды",
                _ => " звёзд",
            });
        }
    }
}
