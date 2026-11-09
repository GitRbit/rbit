//! German (`de`) entries for `catalog/collab/filters.rs`.

use crate::catalog::collab::filters::{IssueFilterMenu, IssueFilterMessage};
use crate::catalog::fallback::MessageWriter;

entries! { De:
    IssueFilterMessage => MessageWriter |text| {
        IssueFilterMessage::Menu(menu) => text.literal(menu_name(menu)),
        IssueFilterMessage::Find(menu) => {
            text.literal(menu_name(menu));
            text.literal(" suchen");
        }
        IssueFilterMessage::FindPlaceholder(menu) => {
            text.literal(menu_name(menu));
            text.literal(" suchen…");
        }
        IssueFilterMessage::NothingToChoose => text.literal("Noch nichts zur Auswahl."),
        IssueFilterMessage::NoMatches => text.literal("Keine Treffer."),
        IssueFilterMessage::Anyone => text.literal("Alle"),
        IssueFilterMessage::Nobody => text.literal("Niemand"),
        IssueFilterMessage::You => text.literal("Sie"),
        IssueFilterMessage::Any => text.literal("Beliebig"),
        IssueFilterMessage::NoMilestone => text.literal("Kein Meilenstein"),
        IssueFilterMessage::AnyLabel => text.literal("Beliebiges Label"),
        IssueFilterMessage::NoLabels => text.literal("Keine Labels"),
        IssueFilterMessage::Open => text.literal("Offen"),
        IssueFilterMessage::Closed => text.literal("Geschlossen"),
        IssueFilterMessage::AllStates => text.literal("Alle Zustände"),
        IssueFilterMessage::AnyStatus => text.literal("Beliebiger Status"),
        IssueFilterMessage::Draft => text.literal("Entwurf"),
        IssueFilterMessage::Merged => text.literal("Zusammengeführt"),
        IssueFilterMessage::SortNewest => text.literal("Neueste"),
        IssueFilterMessage::SortOldest => text.literal("Älteste"),
        IssueFilterMessage::SortRecentlyUpdated => text.literal("Zuletzt aktualisiert"),
        IssueFilterMessage::SortLeastRecentlyUpdated => text.literal("Am längsten nicht aktualisiert"),
        IssueFilterMessage::SortMostCommented => text.literal("Meiste Kommentare"),
    }
}

/// The menu's name, which also starts its search label („Autor suchen“).
const fn menu_name(menu: IssueFilterMenu) -> &'static str {
    match menu {
        IssueFilterMenu::State => "Zustand",
        IssueFilterMenu::Status => "Status",
        IssueFilterMenu::Author => "Autor",
        IssueFilterMenu::Label => "Label",
        IssueFilterMenu::Milestone => "Meilenstein",
        IssueFilterMenu::Assignee => "Zugewiesen",
        IssueFilterMenu::Sort => "Sortierung",
    }
}
