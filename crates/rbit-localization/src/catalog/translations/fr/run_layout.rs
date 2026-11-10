//! French (`fr`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;
use crate::plural::Plural;

entries! { Fr:
    RunLayoutMessage<'a> => MessageWriter |text| {
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
        RunLayoutMessage::Graph => text.literal("Graphe des jobs"),
        RunLayoutMessage::MoreJobs { count } => {
            text.literal("+");
            text.number(count);
            text.literal(" de plus");
        }
        RunLayoutMessage::Annotations => text.literal("Annotations"),
        RunLayoutMessage::Errors { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " erreur",
                _ => " erreurs",
            });
        }
        RunLayoutMessage::Warnings { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " avertissement",
                _ => " avertissements",
            });
        }
        RunLayoutMessage::Notices { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " notification",
                _ => " notifications",
            });
        }
        RunLayoutMessage::ErrorLevel => text.literal("Erreur"),
        RunLayoutMessage::WarningLevel => text.literal("Avertissement"),
        RunLayoutMessage::NoticeLevel => text.literal("Notification"),
        RunLayoutMessage::StepFailed { step } => {
            text.literal("L’étape ");
            text.argument(step);
            text.literal(" a échoué.");
        }
        RunLayoutMessage::MoreAnnotations { count } => {
            text.literal("Et ");
            text.number(count);
            text.literal(" de plus dans les journaux des jobs.");
        }
        RunLayoutMessage::TriggeredVia => text.literal("Déclenchée par"),
        RunLayoutMessage::Status => text.literal("Statut"),
        RunLayoutMessage::TotalDuration => text.literal("Durée totale"),
        RunLayoutMessage::RerunBy { name } => {
            text.literal("Relancée par ");
            text.argument(name);
        }
        RunLayoutMessage::Inputs => text.literal("Entrées"),
        RunLayoutMessage::Waited { time } => {
            text.literal("a attendu ");
            text.argument(time);
        }
        RunLayoutMessage::RanOn { runner } => {
            text.literal("sur ");
            text.argument(runner);
        }
    }
}
