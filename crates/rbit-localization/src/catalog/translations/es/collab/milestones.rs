//! Spanish (`es`) entries for `catalog/collab/milestones.rs`.

use crate::catalog::collab::milestones::MilestoneMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Es:
    MilestoneMessage<'a> => MessageWriter |text| {
        MilestoneMessage::NewMilestone => text.literal("Nuevo hito"),
        MilestoneMessage::EditMilestone => text.literal("Editar hito"),
        MilestoneMessage::EmptyTitle => text.literal("No hay hitos aquí"),
        MilestoneMessage::EmptyText => text.literal(
            "Los hitos agrupan incidencias de cara a una versión o una fecha y muestran cuánto han avanzado.",
        ),
        MilestoneMessage::ClosedWhen { when } => {
            text.literal("Cerrado ");
            text.argument(when);
        }
        MilestoneMessage::DueBy { due } => {
            text.literal("Vence el ");
            text.argument(due);
        }
        MilestoneMessage::PastDueBy { due } => {
            text.literal("Venció el ");
            text.argument(due);
        }
        MilestoneMessage::NoDueDate => text.literal("Sin fecha de vencimiento"),
        MilestoneMessage::Close => text.literal("Cerrar"),
        MilestoneMessage::Reopen => text.literal("Reabrir"),
        MilestoneMessage::DeleteConfirm { title } => {
            text.literal("¿Eliminar el hito «");
            text.argument(title);
            text.literal("»? Sus incidencias se conservan.");
        }
        MilestoneMessage::SearchIssues => text.literal("Buscar en estas incidencias"),
        MilestoneMessage::NothingHereTitle => text.literal("No hay nada aquí"),
        MilestoneMessage::NothingHereText => text.literal(
            "Aquí aparecen las incidencias y solicitudes de cambios añadidas a este hito.",
        ),
        MilestoneMessage::FormIntro => text.literal("Agrupa incidencias de cara a una versión o una fecha."),
        MilestoneMessage::DueDate => text.literal("Fecha de vencimiento"),
        MilestoneMessage::MarkdownOptional => text.literal("Markdown, opcional"),
        MilestoneMessage::CreateMilestone => text.literal("Crear hito"),
        MilestoneMessage::SaveMilestone => text.literal("Guardar hito"),
        MilestoneMessage::Complete { percent } => {
            text.number(percent);
            text.literal(" % completado");
        }
        MilestoneMessage::OpenIssues { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " abierta",
                _ => " abiertas",
            });
        }
        MilestoneMessage::ClosedIssues { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " cerrada",
                _ => " cerradas",
            });
        }
    }
}
