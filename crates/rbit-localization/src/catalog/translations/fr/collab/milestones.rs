//! French (`fr`) entries for `catalog/collab/milestones.rs`.

use crate::catalog::collab::milestones::MilestoneMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Fr:
    MilestoneMessage<'a> => MessageWriter |text| {
        MilestoneMessage::NewMilestone => text.literal("Nouveau jalon"),
        MilestoneMessage::EditMilestone => text.literal("Modifier le jalon"),
        MilestoneMessage::EmptyTitle => text.literal("Aucun jalon ici"),
        MilestoneMessage::EmptyText => text.literal(
            "Les jalons regroupent des tickets en vue d’une release ou d’une date et montrent leur état d’avancement.",
        ),
        MilestoneMessage::ClosedWhen { when } => {
            text.literal("Fermé ");
            text.argument(when);
        }
        MilestoneMessage::DueBy { due } => {
            text.literal("Échéance\u{a0}: ");
            text.argument(due);
        }
        MilestoneMessage::PastDueBy { due } => {
            text.literal("Échéance dépassée\u{a0}: ");
            text.argument(due);
        }
        MilestoneMessage::NoDueDate => text.literal("Aucune échéance"),
        MilestoneMessage::Close => text.literal("Fermer"),
        MilestoneMessage::Reopen => text.literal("Rouvrir"),
        MilestoneMessage::DeleteConfirm { title } => {
            text.literal("Supprimer le jalon «\u{a0}");
            text.argument(title);
            text.literal("\u{a0}»\u{202f}? Ses tickets sont conservés.");
        }
        MilestoneMessage::SearchIssues => text.literal("Rechercher dans ces tickets"),
        MilestoneMessage::NothingHereTitle => text.literal("Rien ici"),
        MilestoneMessage::NothingHereText => text.literal(
            "Les tickets et les pull requests ajoutés à ce jalon apparaissent ici.",
        ),
        MilestoneMessage::FormIntro => text.literal("Regroupez des tickets en vue d’une release ou d’une date."),
        MilestoneMessage::DueDate => text.literal("Échéance"),
        MilestoneMessage::MarkdownOptional => text.literal("Markdown, facultatif"),
        MilestoneMessage::CreateMilestone => text.literal("Créer le jalon"),
        MilestoneMessage::SaveMilestone => text.literal("Enregistrer le jalon"),
        MilestoneMessage::Complete { percent } => {
            text.number(percent);
            text.literal("\u{a0}% terminé");
        }
        MilestoneMessage::OpenIssues { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " ouvert",
                _ => " ouverts",
            });
        }
        MilestoneMessage::ClosedIssues { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " fermé",
                _ => " fermés",
            });
        }
    }
}
