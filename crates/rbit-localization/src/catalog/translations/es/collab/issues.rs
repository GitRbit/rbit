//! Spanish (`es`) entries for `catalog/collab/issues.rs`.

use crate::catalog::collab::issues::IssueMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Es:
    IssueMessage<'a> => MessageWriter |text| {
        IssueMessage::Issues => text.literal("Incidencias"),
        IssueMessage::Labels => text.literal("Etiquetas"),
        IssueMessage::Milestones => text.literal("Hitos"),
        IssueMessage::NewIssue => text.literal("Nueva incidencia"),
        IssueMessage::SearchIssues => text.literal("Buscar incidencias"),
        IssueMessage::SearchPlaceholder => text.literal("Buscar en títulos y descripciones…"),
        IssueMessage::Search => text.literal("Buscar"),
        IssueMessage::SearchHelp => text.literal("Ayuda de búsqueda"),
        IssueMessage::ClearSearch => text.literal("Borrar la búsqueda, los filtros y el orden"),
        IssueMessage::StateNav => text.literal("Estado"),
        IssueMessage::NoIssuesTitle => text.literal("Ninguna incidencia coincide"),
        IssueMessage::NoIssuesText => text.literal("Prueba otra búsqueda o abre la primera incidencia."),
        IssueMessage::DescriptionHint => text.literal("Markdown; #12 y @nombre crean enlaces"),
        IssueMessage::NoLabelsYet => text.literal("Aún no hay etiquetas."),
        IssueMessage::Assignees => text.literal("Asignados"),
        IssueMessage::Milestone => text.literal("Hito"),
        IssueMessage::NoMilestone => text.literal("Sin hito"),
        IssueMessage::OpenIssue => text.literal("Abrir incidencia"),
        IssueMessage::Details => text.literal("Detalles"),
        IssueMessage::Locked => text.literal("bloqueada"),
        IssueMessage::AssignedTo { names } => {
            text.literal("Asignada a ");
            text.argument(names);
        }
        IssueMessage::Comments { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " comentario",
                _ => " comentarios",
            });
        }
    }
}
