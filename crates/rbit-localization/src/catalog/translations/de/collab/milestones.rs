//! German (`de`) entries for `catalog/collab/milestones.rs`.

use crate::catalog::collab::milestones::MilestoneMessage;
use crate::catalog::fallback::MessageWriter;

entries! { De:
    MilestoneMessage<'a> => MessageWriter |text| {
        MilestoneMessage::NewMilestone => text.literal("Neuer Meilenstein"),
        MilestoneMessage::EditMilestone => text.literal("Meilenstein bearbeiten"),
        MilestoneMessage::EmptyTitle => text.literal("Keine Meilensteine vorhanden"),
        MilestoneMessage::EmptyText => text.literal(
            "Meilensteine fassen Issues für ein Release oder einen Termin zusammen und zeigen den Fortschritt.",
        ),
        MilestoneMessage::ClosedWhen { when } => {
            text.literal("Geschlossen ");
            text.argument(when);
        }
        MilestoneMessage::DueBy { due } => {
            text.literal("Fällig am ");
            text.argument(due);
        }
        MilestoneMessage::PastDueBy { due } => {
            text.literal("Überfällig seit ");
            text.argument(due);
        }
        MilestoneMessage::NoDueDate => text.literal("Kein Fälligkeitsdatum"),
        MilestoneMessage::Close => text.literal("Schließen"),
        MilestoneMessage::Reopen => text.literal("Erneut öffnen"),
        MilestoneMessage::DeleteConfirm { title } => {
            text.literal("Meilenstein „");
            text.argument(title);
            text.literal("“ löschen? Seine Issues bleiben erhalten.");
        }
        MilestoneMessage::SearchIssues => text.literal("Diese Issues durchsuchen"),
        MilestoneMessage::NothingHereTitle => text.literal("Noch nichts hier"),
        MilestoneMessage::NothingHereText => {
            text.literal("Issues und Pull-Requests, die diesem Meilenstein hinzugefügt werden, erscheinen hier.");
        }
        MilestoneMessage::FormIntro => text.literal("Fassen Sie Issues für ein Release oder einen Termin zusammen."),
        MilestoneMessage::DueDate => text.literal("Fälligkeitsdatum"),
        MilestoneMessage::MarkdownOptional => text.literal("Markdown, optional"),
        MilestoneMessage::CreateMilestone => text.literal("Meilenstein erstellen"),
        MilestoneMessage::SaveMilestone => text.literal("Meilenstein speichern"),
        MilestoneMessage::Complete { percent } => {
            text.number(percent);
            text.literal("\u{a0}% abgeschlossen");
        }
        MilestoneMessage::OpenIssues { count } => {
            text.number(count);
            text.literal(" offen");
        }
        MilestoneMessage::ClosedIssues { count } => {
            text.number(count);
            text.literal(" geschlossen");
        }
    }
}
