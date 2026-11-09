//! Spanish (`es`) entries for `catalog/collab/merge.rs`.

use crate::catalog::collab::merge::MergeMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Es:
    MergeMessage<'a> => MessageWriter |text| {
        MergeMessage::Merge => text.literal("Fusionar"),
        MergeMessage::ConflictingFiles => text.literal("Archivos en conflicto"),
        MergeMessage::WorkInProgress => text.literal("Esta solicitud de cambios todavía está en curso."),
        MergeMessage::ReadyForReview => text.literal("Lista para revisión"),
        MergeMessage::MergeMethod => text.literal("Método de fusión"),
        MergeMessage::CreateMergeCommit => text.literal("Crear un commit de fusión"),
        MergeMessage::SquashAndMerge => text.literal("Fusionar con squash"),
        MergeMessage::RebaseAndMerge => text.literal("Fusionar con rebase"),
        MergeMessage::EditMessage => text.literal("Editar el mensaje del commit"),
        MergeMessage::Message => text.literal("Mensaje"),
        MergeMessage::TitlePlaceholder { title, squash } => {
            text.argument(title);
            text.literal(" (o, con squash, ");
            text.argument(squash);
            text.literal(")");
        }
        MergeMessage::DeleteHead => text.literal("Eliminar la rama de origen después de fusionar"),
        MergeMessage::Bypass => text.literal(
            "Fusionar sin cumplir los requisitos de protección de la rama (queda registrado en la auditoría)",
        ),
        MergeMessage::MergePull => text.literal("Fusionar la solicitud de cambios"),
        MergeMessage::WriteAccessOnly => {
            text.literal("Solo las personas con acceso de escritura a este repositorio pueden fusionar.");
        }
        MergeMessage::PullOpen => text.literal("La solicitud de cambios está abierta"),
        MergeMessage::PullClosed => text.literal("La solicitud de cambios está cerrada"),
        MergeMessage::StillDraft => text.literal("Esta solicitud de cambios todavía es un borrador"),
        MergeMessage::HeadExists => text.literal("La rama de origen existe"),
        MergeMessage::HeadDeleted => text.literal("Se eliminó la rama de origen"),
        MergeMessage::NoConflicts => text.literal("No hay conflictos con la rama base"),
        MergeMessage::Conflicts => text.literal("Esta rama tiene conflictos que hay que resolver"),
        MergeMessage::CheckingConflicts => text.literal("Buscando conflictos…"),
        MergeMessage::MergeabilityError => text.literal("No se pudo comprobar si se puede fusionar"),
        MergeMessage::Approvals { have, need } => {
            text.number(have);
            text.literal(" de ");
            text.number(need);
            text.literal(match text.plural(need) {
                Plural::One => " aprobación obligatoria",
                _ => " aprobaciones obligatorias",
            });
        }
        MergeMessage::NoChangesRequested => text.literal("No se solicitaron cambios"),
        MergeMessage::ChangesRequestedBy { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " revisor solicitó cambios",
                _ => " revisores solicitaron cambios",
            });
        }
        MergeMessage::MayPush => text.literal("Puedes enviar a la rama base"),
        MergeMessage::MayNotPush => text.literal("No tienes permiso para enviar a la rama base"),
        MergeMessage::NoChecks => text.literal("No hay comprobaciones de estado"),
        MergeMessage::ChecksPending => text.literal("Algunas comprobaciones aún no terminaron"),
        MergeMessage::ChecksPassed => text.literal("Todas las comprobaciones se superaron"),
        MergeMessage::ChecksFailed => text.literal("Algunas comprobaciones no fueron correctas"),
        MergeMessage::RequiredPassed { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " comprobación obligatoria superada",
                _ => " comprobaciones obligatorias superadas",
            });
        }
        MergeMessage::RequiredFailed { failing, required } => {
            text.literal("Fallaron ");
            text.number(failing);
            text.literal(" de ");
            text.number(required);
            text.literal(" comprobaciones obligatorias");
        }
        MergeMessage::RequiredWaiting { waiting, required } => {
            text.literal("Esperando ");
            text.number(waiting);
            text.literal(" de ");
            text.number(required);
            text.literal(" comprobaciones obligatorias");
        }
    }
}
