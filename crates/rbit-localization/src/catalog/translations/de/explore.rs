//! German (`de`) entries for `catalog/explore.rs`.

use crate::catalog::RepositoryVisibility;
use crate::catalog::explore::{Message, signed};
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { De:
    Message<'a> => MessageWriter |text| {
        Message::RepositoriesTitle => text.literal("Repositorys"),
        Message::RepositoryCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Repository",
                _ => " Repositorys",
            });
        }
        Message::OwnerRepositories { owner } => {
            text.literal("Repositorys von ");
            text.argument(owner);
        }
        Message::Visibility { visibility } => text.literal(match visibility {
            RepositoryVisibility::Public => "Öffentlich",
            RepositoryVisibility::Private => "Privat",
            RepositoryVisibility::Internal => "Intern",
        }),
        Message::EmptyRepositories => text.literal("Keine Repositorys gefunden."),
        Message::SearchRepositories => text.literal("Repositorys durchsuchen"),
        Message::SearchLabel => text.literal("Suchen"),
        Message::NextPage => text.literal("Weiter"),
        Message::PreviousPage => text.literal("Zurück"),
        Message::PageNumber { page } => {
            text.literal("Seite ");
            text.number(page);
        }
        Message::RepositoryLinkLabel { owner, name } => {
            text.literal("Repository ");
            text.argument(owner);
            text.literal("/");
            text.argument(name);
            text.literal(" öffnen");
        }
        Message::ExploreTitle => text.literal("Entdecken"),
        Message::ExploreSubtitle { site } => {
            text.literal("Öffentliche Repositorys auf ");
            text.argument(site);
            text.literal(".");
        }
        Message::ExploreSearchPlaceholder => text.literal("Nach Name oder Beschreibung suchen"),
        Message::SortBy => text.literal("Sortieren nach"),
        Message::SortUpdated => text.literal("Aktualisiert"),
        Message::SortName => text.literal("Name"),
        Message::SortNewest => text.literal("Neueste"),
        Message::ArchiveFilterLabel => text.literal("Archivierte Repositorys"),
        Message::ArchiveAll => text.literal("Alle"),
        Message::ArchiveActive => text.literal("Aktiv"),
        Message::Archived => text.literal("Archiviert"),
        Message::NothingHereYet => text.literal("Noch nichts vorhanden"),
        Message::PublicRepositoriesEmpty => {
            text.literal("Öffentliche Repositorys werden hier aufgeführt, sobald es welche gibt.");
        }
        Message::NoMatchingRepositories => text.literal("Keine passenden Repositorys"),
        Message::SearchSuggestion => text.literal("Versuchen Sie eine kürzere Suche oder andere Wörter."),
        Message::CountForQuery { count, query } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Repository",
                _ => " Repositorys",
            });
            if !query.is_empty() {
                text.literal(" für „");
                text.argument(query);
                text.literal("“");
            }
        }
        Message::PaginationLabel => text.literal("Seitennavigation"),
        Message::PageOf { page, pages } => {
            text.literal("Seite ");
            text.number(page.into());
            text.literal(" von ");
            text.number(pages.into());
        }
        Message::Stars { count } => {
            signed(text, count);
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => " Stern",
                _ => " Sterne",
            });
        }
    }
}
