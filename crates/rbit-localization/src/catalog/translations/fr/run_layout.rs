//! French (`fr`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;
use crate::plural::Plural;

entries! { Fr:
    RunLayoutMessage => MessageWriter |text| {
        RunLayoutMessage::Summary => text.literal("Résumé"),
        RunLayoutMessage::Navigation => text.literal("Navigation de l’exécution"),
        RunLayoutMessage::Usage => text.literal("Utilisation"),
        RunLayoutMessage::WorkflowFile => text.literal("Fichier du workflow"),
        RunLayoutMessage::JobCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " job",
                _ => " jobs",
            });
        }
    }
}
