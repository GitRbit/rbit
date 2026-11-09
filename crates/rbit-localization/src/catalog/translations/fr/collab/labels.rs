//! French (`fr`) entries for `catalog/collab/labels.rs`.

use crate::catalog::collab::labels::LabelMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Fr:
    LabelMessage<'a> => MessageWriter |text| {
        LabelMessage::NewLabel => text.literal("Nouvelle étiquette"),
        LabelMessage::Name => text.literal("Nom"),
        LabelMessage::Colour => text.literal("Couleur"),
        LabelMessage::CreateLabel => text.literal("Créer l’étiquette"),
        LabelMessage::Count { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " étiquette",
                _ => " étiquettes",
            });
        }
        LabelMessage::EmptyTitle => text.literal("Aucune étiquette pour l’instant"),
        LabelMessage::EmptyText => text.literal(
            "Les étiquettes classent les tickets et les pull requests\u{a0}: bug, amélioration, bon premier ticket…",
        ),
        LabelMessage::CreateDefaults => text.literal("Créer les étiquettes par défaut"),
        LabelMessage::OpenIssues { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " ticket ouvert",
                _ => " tickets ouverts",
            });
        }
        LabelMessage::DeleteConfirm { name } => {
            text.literal("Supprimer l’étiquette «\u{a0}");
            text.argument(name);
            text.literal("\u{a0}»\u{202f}? Elle sera retirée de tous les tickets.");
        }
    }
}
