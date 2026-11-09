//! German (`de`) entries for `catalog/collab/issues.rs`.

use crate::catalog::collab::issues::IssueMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { De:
    IssueMessage<'a> => MessageWriter |text| {
        IssueMessage::Issues => text.literal("Issues"),
        IssueMessage::Labels => text.literal("Labels"),
        IssueMessage::Milestones => text.literal("Meilensteine"),
        IssueMessage::NewIssue => text.literal("Neues Issue"),
        IssueMessage::SearchIssues => text.literal("Issues durchsuchen"),
        IssueMessage::SearchPlaceholder => text.literal("Titel und Beschreibungen durchsuchen…"),
        IssueMessage::Search => text.literal("Suchen"),
        IssueMessage::SearchHelp => text.literal("Hilfe zur Suche"),
        IssueMessage::ClearSearch => text.literal("Suche, Filter und Sortierung zurücksetzen"),
        IssueMessage::StateNav => text.literal("Zustand"),
        IssueMessage::NoIssuesTitle => text.literal("Keine passenden Issues"),
        IssueMessage::NoIssuesText => text.literal("Versuchen Sie eine andere Suche oder eröffnen Sie das erste Issue."),
        IssueMessage::DescriptionHint => text.literal("Markdown; #12 und @name werden verlinkt"),
        IssueMessage::NoLabelsYet => text.literal("Noch keine Labels."),
        IssueMessage::Assignees => text.literal("Zugewiesene"),
        IssueMessage::Milestone => text.literal("Meilenstein"),
        IssueMessage::NoMilestone => text.literal("Kein Meilenstein"),
        IssueMessage::OpenIssue => text.literal("Issue eröffnen"),
        IssueMessage::Details => text.literal("Details"),
        IssueMessage::Locked => text.literal("gesperrt"),
        IssueMessage::AssignedTo { names } => {
            text.literal("Zugewiesen an ");
            text.argument(names);
        }
        IssueMessage::Comments { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Kommentar",
                _ => " Kommentare",
            });
        }
    }
}
