//! French (`fr`) entries for `catalog/explore.rs`.

use crate::catalog::RepositoryVisibility;
use crate::catalog::explore::{Message, signed};
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Fr:
    Message<'a> => MessageWriter |text| {
        Message::RepositoriesTitle => text.literal("Dépôts"),
        Message::RepositoryCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " dépôt",
                _ => " dépôts",
            });
        }
        Message::OwnerRepositories { owner } => {
            text.literal("Dépôts de ");
            text.argument(owner);
        }
        Message::Visibility { visibility } => text.literal(match visibility {
            RepositoryVisibility::Public => "Public",
            RepositoryVisibility::Private => "Privé",
            RepositoryVisibility::Internal => "Interne",
        }),
        Message::EmptyRepositories => text.literal("Aucun dépôt trouvé."),
        Message::SearchRepositories => text.literal("Rechercher des dépôts"),
        Message::SearchLabel => text.literal("Rechercher"),
        Message::NextPage => text.literal("Suivant"),
        Message::PreviousPage => text.literal("Précédent"),
        Message::PageNumber { page } => {
            text.literal("Page ");
            text.number(page);
        }
        Message::RepositoryLinkLabel { owner, name } => {
            text.literal("Ouvrir le dépôt ");
            text.argument(owner);
            text.literal("/");
            text.argument(name);
        }
        Message::ExploreTitle => text.literal("Explorer"),
        Message::ExploreSubtitle { site } => {
            text.literal("Dépôts publics sur ");
            text.argument(site);
            text.literal(".");
        }
        Message::ExploreSearchPlaceholder => text.literal("Rechercher par nom ou description"),
        Message::SortBy => text.literal("Trier par"),
        Message::SortUpdated => text.literal("Mis à jour"),
        Message::SortName => text.literal("Nom"),
        Message::SortNewest => text.literal("Plus récents"),
        Message::ArchiveFilterLabel => text.literal("Dépôts archivés"),
        Message::ArchiveAll => text.literal("Tous"),
        Message::ArchiveActive => text.literal("Actifs"),
        Message::Archived => text.literal("Archivé"),
        Message::NothingHereYet => text.literal("Rien ici pour l’instant"),
        Message::PublicRepositoriesEmpty => {
            text.literal("Les dépôts publics apparaîtront ici dès qu’il y en aura.");
        }
        Message::NoMatchingRepositories => text.literal("Aucun dépôt correspondant"),
        Message::SearchSuggestion => text.literal("Essayez une recherche plus courte ou d’autres mots."),
        Message::CountForQuery { count, query } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " dépôt",
                _ => " dépôts",
            });
            if !query.is_empty() {
                text.literal(" pour «\u{a0}");
                text.argument(query);
                text.literal("\u{a0}»");
            }
        }
        Message::PaginationLabel => text.literal("Pagination"),
        Message::PageOf { page, pages } => {
            text.literal("Page ");
            text.number(page.into());
            text.literal(" sur ");
            text.number(pages.into());
        }
        Message::Stars { count } => {
            signed(text, count);
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => " étoile",
                _ => " étoiles",
            });
        }
    }
}
