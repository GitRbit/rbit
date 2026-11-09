//! Brazilian Portuguese (`pt-BR`) entries for `catalog/explore.rs`.

use crate::catalog::RepositoryVisibility;
use crate::catalog::explore::{Message, signed};
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { PtBr:
    Message<'a> => MessageWriter |text| {
        Message::RepositoriesTitle => text.literal("Repositórios"),
        Message::RepositoryCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " repositório",
                _ => " repositórios",
            });
        }
        Message::OwnerRepositories { owner } => {
            text.literal("Repositórios de ");
            text.argument(owner);
        }
        Message::Visibility { visibility } => text.literal(match visibility {
            RepositoryVisibility::Public => "Público",
            RepositoryVisibility::Private => "Privado",
            RepositoryVisibility::Internal => "Interno",
        }),
        Message::EmptyRepositories => text.literal("Nenhum repositório encontrado."),
        Message::SearchRepositories => text.literal("Pesquisar repositórios"),
        Message::SearchLabel => text.literal("Pesquisar"),
        Message::NextPage => text.literal("Próxima"),
        Message::PreviousPage => text.literal("Anterior"),
        Message::PageNumber { page } => {
            text.literal("Página ");
            text.number(page);
        }
        Message::RepositoryLinkLabel { owner, name } => {
            text.literal("Abrir repositório ");
            text.argument(owner);
            text.literal("/");
            text.argument(name);
        }
        Message::ExploreTitle => text.literal("Explorar"),
        Message::ExploreSubtitle { site } => {
            text.literal("Repositórios públicos em ");
            text.argument(site);
            text.literal(".");
        }
        Message::ExploreSearchPlaceholder => text.literal("Pesquisar por nome ou descrição"),
        Message::SortBy => text.literal("Ordenar por"),
        Message::SortUpdated => text.literal("Atualizados"),
        Message::SortName => text.literal("Nome"),
        Message::SortNewest => text.literal("Mais recentes"),
        Message::ArchiveFilterLabel => text.literal("Repositórios arquivados"),
        Message::ArchiveAll => text.literal("Todos"),
        Message::ArchiveActive => text.literal("Ativos"),
        Message::Archived => text.literal("Arquivado"),
        Message::NothingHereYet => text.literal("Nada aqui ainda"),
        Message::PublicRepositoriesEmpty => text.literal("Os repositórios públicos aparecerão aqui quando existirem."),
        Message::NoMatchingRepositories => text.literal("Nenhum repositório correspondente"),
        Message::SearchSuggestion => text.literal("Tente uma pesquisa mais curta ou outras palavras."),
        Message::CountForQuery { count, query } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " repositório",
                _ => " repositórios",
            });
            if !query.is_empty() {
                text.literal(" para “");
                text.argument(query);
                text.literal("”");
            }
        }
        Message::PaginationLabel => text.literal("Paginação"),
        Message::PageOf { page, pages } => {
            text.literal("Página ");
            text.number(page.into());
            text.literal(" de ");
            text.number(pages.into());
        }
        Message::Stars { count } => {
            signed(text, count);
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => " estrela",
                _ => " estrelas",
            });
        }
    }
}
