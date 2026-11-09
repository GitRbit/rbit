//! Spanish (`es`) entries for `catalog/explore.rs`.

use crate::catalog::RepositoryVisibility;
use crate::catalog::explore::{Message, signed};
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Es:
    Message<'a> => MessageWriter |text| {
        Message::RepositoriesTitle => text.literal("Repositorios"),
        Message::RepositoryCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " repositorio",
                _ => " repositorios",
            });
        }
        Message::OwnerRepositories { owner } => {
            text.literal("Repositorios de ");
            text.argument(owner);
        }
        Message::Visibility { visibility } => text.literal(match visibility {
            RepositoryVisibility::Public => "Público",
            RepositoryVisibility::Private => "Privado",
            RepositoryVisibility::Internal => "Interno",
        }),
        Message::EmptyRepositories => text.literal("No se encontraron repositorios."),
        Message::SearchRepositories => text.literal("Buscar repositorios"),
        Message::SearchLabel => text.literal("Buscar"),
        Message::NextPage => text.literal("Siguiente"),
        Message::PreviousPage => text.literal("Anterior"),
        Message::PageNumber { page } => {
            text.literal("Página ");
            text.number(page);
        }
        Message::RepositoryLinkLabel { owner, name } => {
            text.literal("Abrir repositorio ");
            text.argument(owner);
            text.literal("/");
            text.argument(name);
        }
        Message::ExploreTitle => text.literal("Explorar"),
        Message::ExploreSubtitle { site } => {
            text.literal("Repositorios públicos en ");
            text.argument(site);
            text.literal(".");
        }
        Message::ExploreSearchPlaceholder => text.literal("Buscar por nombre o descripción"),
        Message::SortBy => text.literal("Ordenar por"),
        Message::SortUpdated => text.literal("Actualizados"),
        Message::SortName => text.literal("Nombre"),
        Message::SortNewest => text.literal("Más recientes"),
        Message::ArchiveFilterLabel => text.literal("Repositorios archivados"),
        Message::ArchiveAll => text.literal("Todos"),
        Message::ArchiveActive => text.literal("Activos"),
        Message::Archived => text.literal("Archivado"),
        Message::NothingHereYet => text.literal("Aún no hay nada aquí"),
        Message::PublicRepositoriesEmpty => text.literal("Los repositorios públicos aparecerán aquí cuando existan."),
        Message::NoMatchingRepositories => text.literal("No hay repositorios coincidentes"),
        Message::SearchSuggestion => text.literal("Prueba una búsqueda más corta u otras palabras."),
        Message::CountForQuery { count, query } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " repositorio",
                _ => " repositorios",
            });
            if !query.is_empty() {
                text.literal(" para «");
                text.argument(query);
                text.literal("»");
            }
        }
        Message::PaginationLabel => text.literal("Paginación"),
        Message::PageOf { page, pages } => {
            text.literal("Página ");
            text.number(page.into());
            text.literal(" de ");
            text.number(pages.into());
        }
        Message::Stars { count } => {
            signed(text, count);
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => " estrella",
                _ => " estrellas",
            });
        }
    }
}
