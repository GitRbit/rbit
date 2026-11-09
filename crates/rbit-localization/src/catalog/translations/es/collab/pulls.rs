//! Spanish (`es`) entries for `catalog/collab/pulls.rs`.

use crate::catalog::collab::pulls::PullMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Es:
    PullMessage<'a> => MessageWriter |text| {
        PullMessage::PullRequests => text.literal("Solicitudes de cambios"),
        PullMessage::SearchPulls => text.literal("Buscar solicitudes de cambios"),
        PullMessage::ClearSearch => text.literal("Borrar la búsqueda y los filtros"),
        PullMessage::FiltersLabel => text.literal("Filtros de solicitudes de cambios"),
        PullMessage::StateNav => text.literal("Estado de la solicitud de cambios"),
        PullMessage::AuthorFilter { author } => {
            text.literal("autor: ");
            text.argument(author);
        }
        PullMessage::New => text.literal("Nueva"),
        PullMessage::NewSuffix => text.literal("\u{a0}solicitud de cambios"),
        PullMessage::EmptyTitle => text.literal("No hay solicitudes de cambios aquí"),
        PullMessage::EmptyText => text.literal(
            "Las solicitudes de cambios proponen cambios desde una rama o una bifurcación. Compara ramas para abrir \
             una.",
        ),
        PullMessage::OpenTitle => text.literal("Abrir una solicitud de cambios"),
        PullMessage::OpenIntro => text.literal(
            "Propón los cambios de una rama, de este repositorio o de una bifurcación, para la rama base.",
        ),
        PullMessage::OpenAsDraft => text.literal("Abrir como borrador"),
        PullMessage::Create => text.literal("Crear solicitud de cambios"),
        PullMessage::DifferentHistories => text.literal("Estas ramas tienen historiales completamente distintos."),
        PullMessage::ChooseBranches => text.literal("Elige dos ramas existentes para compararlas."),
        PullMessage::UpToDate { base, head } => {
            text.argument(base);
            text.literal(" ya contiene todos los commits de ");
            text.argument(head);
            text.literal(".");
        }
        PullMessage::BaseRepository => text.literal("Repositorio base"),
        PullMessage::BaseRefs => text.literal("Ramas y tags base"),
        PullMessage::Base => text.literal("Base"),
        PullMessage::HeadRepository => text.literal("Repositorio de origen"),
        PullMessage::Compare => text.literal("Comparar"),
        PullMessage::CompareRefs => text.literal("Comparar ramas y tags"),
        PullMessage::PullRequest => text.literal("Solicitud de cambios"),
        PullMessage::Conversation => text.literal("Conversación"),
        PullMessage::Commits => text.literal("Commits"),
        PullMessage::FilesChanged => text.literal("Archivos modificados"),
        PullMessage::CommitsPhrase { count: Some(count) } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " commit",
                _ => " commits",
            });
        }
        PullMessage::CommitsPhrase { count: None } => text.literal("commits"),
        PullMessage::ChangeBase => text.literal("Cambiar la rama base"),
        PullMessage::BaseBranch => text.literal("Rama base"),
        PullMessage::BaseBranches => text.literal("Ramas base"),
        PullMessage::ChangeBaseHint => text.literal(
            "Al cambiar la base se actualizan los commits, las diferencias y las comprobaciones de fusión.",
        ),
        PullMessage::UpdateBase => text.literal("Actualizar la rama base"),
        PullMessage::IsClosed => text.literal("Esta solicitud de cambios está cerrada."),
        PullMessage::IsMerged => text.literal("Solicitud de cambios fusionada."),
        PullMessage::DeleteBranch { branch } => {
            text.literal("Eliminar ");
            text.argument(branch);
        }
        PullMessage::ConvertToDraft => text.literal("Convertir en borrador"),
        PullMessage::NoCommitsTitle => text.literal("No hay commits"),
        PullMessage::NoCommitsText => text.literal("La rama de origen y la base apuntan al mismo commit."),
        PullMessage::NewestCommits { count } => {
            let one = text.plural(count) == Plural::One;
            text.literal(if one { "Solo se muestra el " } else { "Solo se muestran los " });
            text.number(count);
            text.literal(if one { " commit más reciente." } else { " commits más recientes." });
        }
    }
}
