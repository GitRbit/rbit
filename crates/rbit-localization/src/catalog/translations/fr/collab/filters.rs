//! French (`fr`) entries for `catalog/collab/filters.rs`.

use crate::catalog::collab::filters::{IssueFilterMenu, IssueFilterMessage};
use crate::catalog::fallback::MessageWriter;

entries! { Fr:
    IssueFilterMessage => MessageWriter |text| {
        IssueFilterMessage::Menu(menu) => text.literal(menu_entries(menu)[0]),
        IssueFilterMessage::Find(menu) => text.literal(menu_entries(menu)[1]),
        IssueFilterMessage::FindPlaceholder(menu) => {
            text.literal(menu_entries(menu)[1]);
            text.literal("…");
        }
        IssueFilterMessage::NothingToChoose => text.literal("Rien à choisir pour l’instant."),
        IssueFilterMessage::NoMatches => text.literal("Aucun résultat."),
        IssueFilterMessage::Anyone => text.literal("N’importe qui"),
        IssueFilterMessage::Nobody => text.literal("Personne"),
        IssueFilterMessage::You => text.literal("Vous"),
        IssueFilterMessage::Any => text.literal("Tous"),
        IssueFilterMessage::NoMilestone => text.literal("Aucun jalon"),
        IssueFilterMessage::AnyLabel => text.literal("N’importe quelle étiquette"),
        IssueFilterMessage::NoLabels => text.literal("Aucune étiquette"),
        IssueFilterMessage::Open => text.literal("Ouvert"),
        IssueFilterMessage::Closed => text.literal("Fermé"),
        IssueFilterMessage::AllStates => text.literal("Tous les états"),
        IssueFilterMessage::AnyStatus => text.literal("Tous les statuts"),
        IssueFilterMessage::Draft => text.literal("Brouillon"),
        IssueFilterMessage::Merged => text.literal("Fusionné"),
        IssueFilterMessage::SortNewest => text.literal("Plus récents"),
        IssueFilterMessage::SortOldest => text.literal("Plus anciens"),
        IssueFilterMessage::SortRecentlyUpdated => text.literal("Mis à jour récemment"),
        IssueFilterMessage::SortLeastRecentlyUpdated => text.literal("Mis à jour le moins récemment"),
        IssueFilterMessage::SortMostCommented => text.literal("Les plus commentés"),
    }
}

/// The menu's name and its search label.
const fn menu_entries(menu: IssueFilterMenu) -> [&'static str; 2] {
    match menu {
        IssueFilterMenu::State => ["État", "Rechercher un état"],
        IssueFilterMenu::Status => ["Statut", "Rechercher un statut"],
        IssueFilterMenu::Author => ["Auteur", "Rechercher un auteur"],
        IssueFilterMenu::Label => ["Étiquette", "Rechercher une étiquette"],
        IssueFilterMenu::Milestone => ["Jalon", "Rechercher un jalon"],
        IssueFilterMenu::Assignee => ["Personne assignée", "Rechercher une personne assignée"],
        IssueFilterMenu::Sort => ["Tri", "Rechercher un tri"],
    }
}
