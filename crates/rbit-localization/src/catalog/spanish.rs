//! Spanish base catalog. Integer noun messages explicitly use singular for one;
//! the CLDR many/other categories share these plural noun forms. No date engine.
use super::{Catalog, LocalizedText, Message, RepositoryVisibility};
mod actions;
mod job;
mod overview;
pub(super) use actions::actions;
pub(super) use actions::{RERUN_PREFIX, SUMMARY_PREFIX};
pub(super) use job::job;
pub(super) use overview::overview;

pub(super) const PAGE_PREFIX: &str = "Página ";
pub(super) const PAGE_OF: &str = " de ";
pub(super) const SUBTITLE_PREFIX: &str = "Repositorios públicos en ";

pub(super) fn text(catalog: Catalog, message: Message<'_>) -> LocalizedText {
    let literal = match message {
        Message::RepositoriesTitle => "Repositorios",
        Message::Visibility { visibility } => match visibility {
            RepositoryVisibility::Public => "Público",
            RepositoryVisibility::Private => "Privado",
            RepositoryVisibility::Internal => "Interno",
        },
        Message::EmptyRepositories => "No se encontraron repositorios.",
        Message::SearchRepositories => "Buscar repositorios",
        Message::SearchLabel => "Buscar",
        Message::NextPage => "Siguiente",
        Message::PreviousPage => "Anterior",
        Message::ExploreTitle => "Explorar",
        Message::ExploreSearchPlaceholder => "Buscar por nombre o descripción",
        Message::SortBy => "Ordenar por",
        Message::SortUpdated => "Actualizados",
        Message::SortName => "Nombre",
        Message::SortNewest => "Más recientes",
        Message::ArchiveFilterLabel => "Repositorios archivados",
        Message::ArchiveAll => "Todos",
        Message::ArchiveActive => "Activos",
        Message::Archived => "Archivado",
        Message::NothingHereYet => "Aún no hay nada aquí",
        Message::PublicRepositoriesEmpty => "Los repositorios públicos aparecerán aquí cuando existan.",
        Message::NoMatchingRepositories => "No hay repositorios coincidentes",
        Message::SearchSuggestion => "Prueba una búsqueda más corta u otras palabras.",
        Message::PaginationLabel => "Paginación",
        Message::RepositoryCount { count } => return owned(format!("{count} {}", repositories(count))),
        Message::OwnerRepositories { owner } => return owned(format!("Repositorios de {owner}")),
        Message::PageNumber { page } => return owned(format!("{PAGE_PREFIX}{page}")),
        Message::RepositoryLinkLabel { owner, name } => return owned(format!("Abrir repositorio {owner}/{name}")),
        Message::ExploreSubtitle { site } => return owned(format!("{SUBTITLE_PREFIX}{site}.")),
        Message::CountForQuery { count, query } => {
            let mut output = format!("{count} {}", repositories(count));
            if !query.is_empty() {
                output.push_str(" para «");
                output.push_str(query);
                output.push('»');
            }
            return owned(output);
        }
        Message::PageOf { page, pages } => return owned(format!("{PAGE_PREFIX}{page}{PAGE_OF}{pages}")),
        Message::Stars { count } => return owned(format!("{count} {}", if count.unsigned_abs() == 1 { "estrella" } else { "estrellas" })),
    };
    catalog.literal_text(literal)
}

fn repositories(count: u64) -> &'static str {
    if count == 1 { "repositorio" } else { "repositorios" }
}
fn owned(value: String) -> LocalizedText {
    LocalizedText(value.into())
}
