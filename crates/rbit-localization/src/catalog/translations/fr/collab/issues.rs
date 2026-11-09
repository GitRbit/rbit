//! French (`fr`) entries for `catalog/collab/issues.rs`.

use crate::catalog::collab::issues::IssueMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Fr:
    IssueMessage<'a> => MessageWriter |text| {
        IssueMessage::Issues => text.literal("Tickets"),
        IssueMessage::Labels => text.literal("Étiquettes"),
        IssueMessage::Milestones => text.literal("Jalons"),
        IssueMessage::NewIssue => text.literal("Nouveau ticket"),
        IssueMessage::SearchIssues => text.literal("Rechercher des tickets"),
        IssueMessage::SearchPlaceholder => text.literal("Rechercher dans les titres et les descriptions…"),
        IssueMessage::Search => text.literal("Rechercher"),
        IssueMessage::SearchHelp => text.literal("Aide à la recherche"),
        IssueMessage::ClearSearch => text.literal("Effacer la recherche, les filtres et le tri"),
        IssueMessage::StateNav => text.literal("État"),
        IssueMessage::NoIssuesTitle => text.literal("Aucun ticket ne correspond"),
        IssueMessage::NoIssuesText => text.literal("Essayez une autre recherche ou ouvrez le premier ticket."),
        IssueMessage::DescriptionHint => text.literal("Markdown\u{202f}; #12 et @nom créent des liens"),
        IssueMessage::NoLabelsYet => text.literal("Aucune étiquette pour l’instant."),
        IssueMessage::Assignees => text.literal("Personnes assignées"),
        IssueMessage::Milestone => text.literal("Jalon"),
        IssueMessage::NoMilestone => text.literal("Aucun jalon"),
        IssueMessage::OpenIssue => text.literal("Ouvrir le ticket"),
        IssueMessage::Details => text.literal("Détails"),
        IssueMessage::Locked => text.literal("Verrouillé"),
        IssueMessage::AssignedTo { names } => {
            text.literal("Assigné à ");
            text.argument(names);
        }
        IssueMessage::Comments { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " commentaire",
                _ => " commentaires",
            });
        }
    }
}
