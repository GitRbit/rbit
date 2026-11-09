//! Spanish (`es`) entries for `catalog/collab/filters.rs`.

use crate::catalog::collab::filters::{IssueFilterMenu, IssueFilterMessage};
use crate::catalog::fallback::MessageWriter;

entries! { Es:
    IssueFilterMessage => MessageWriter |text| {
        IssueFilterMessage::Menu(menu) => text.literal(names(menu).0),
        IssueFilterMessage::Find(menu) => {
            text.literal("Buscar ");
            text.literal(names(menu).1);
        }
        IssueFilterMessage::FindPlaceholder(menu) => {
            text.literal("Buscar ");
            text.literal(names(menu).1);
            text.literal("…");
        }
        IssueFilterMessage::NothingToChoose => text.literal("Aún no hay nada que elegir."),
        IssueFilterMessage::NoMatches => text.literal("No hay coincidencias."),
        IssueFilterMessage::Anyone => text.literal("Cualquiera"),
        IssueFilterMessage::Nobody => text.literal("Nadie"),
        IssueFilterMessage::You => text.literal("Tú"),
        IssueFilterMessage::Any => text.literal("Cualquiera"),
        IssueFilterMessage::NoMilestone => text.literal("Sin hito"),
        IssueFilterMessage::AnyLabel => text.literal("Cualquier etiqueta"),
        IssueFilterMessage::NoLabels => text.literal("Sin etiquetas"),
        IssueFilterMessage::Open => text.literal("Abiertas"),
        IssueFilterMessage::Closed => text.literal("Cerradas"),
        IssueFilterMessage::AllStates => text.literal("Todos los estados"),
        IssueFilterMessage::AnyStatus => text.literal("Cualquier situación"),
        IssueFilterMessage::Draft => text.literal("Borrador"),
        IssueFilterMessage::Merged => text.literal("Fusionadas"),
        IssueFilterMessage::SortNewest => text.literal("Más recientes"),
        IssueFilterMessage::SortOldest => text.literal("Más antiguas"),
        IssueFilterMessage::SortRecentlyUpdated => text.literal("Actualizadas recientemente"),
        IssueFilterMessage::SortLeastRecentlyUpdated => text.literal("Actualizadas hace más tiempo"),
        IssueFilterMessage::SortMostCommented => text.literal("Más comentadas"),
    }
}

/// A menu's name and the noun its search box looks for.
const fn names(menu: IssueFilterMenu) -> (&'static str, &'static str) {
    match menu {
        IssueFilterMenu::State => ("Estado", "estado"),
        IssueFilterMenu::Status => ("Situación", "situación"),
        IssueFilterMenu::Author => ("Autor", "autor"),
        IssueFilterMenu::Label => ("Etiqueta", "etiqueta"),
        IssueFilterMenu::Milestone => ("Hito", "hito"),
        IssueFilterMenu::Assignee => ("Asignado a", "persona asignada"),
        IssueFilterMenu::Sort => ("Ordenar", "orden"),
    }
}
