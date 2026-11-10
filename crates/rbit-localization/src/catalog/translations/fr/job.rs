//! French (`fr`) entries for `catalog/job.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::job::ActionsJobMessage;
use crate::plural::Plural;

entries! { Fr:
    ActionsJobMessage => MessageWriter |text| {
        ActionsJobMessage::Details => text.literal("Détails du job"),
        ActionsJobMessage::Steps => text.literal("Étapes et journaux"),
        ActionsJobMessage::RunsOn => text.literal("s’exécute sur"),
        ActionsJobMessage::Duration => text.literal("Durée"),
        ActionsJobMessage::RawLog => text.literal("Journal brut"),
        ActionsJobMessage::Rerun => text.literal("Relancer le job"),
        ActionsJobMessage::NotStarted => text.literal("Pas encore démarré"),
        ActionsJobMessage::NotStartedDescription => text.literal(
            "Les étapes et leurs journaux apparaîtront ici dès qu’un runner prendra ce job en charge.",
        ),
        ActionsJobMessage::RetainedWindow => text.literal(
            "Les aperçus des étapes sont limités. Ouvrez une étape pour voir plus de lignes ou téléchargez le \
             journal brut.",
        ),
        ActionsJobMessage::Expired => text.literal("Ce journal a été supprimé par la règle de conservation des journaux."),
        ActionsJobMessage::NoOutput => text.literal("Aucune sortie."),
        ActionsJobMessage::EarlierLines => text.literal("Les lignes précédentes sont omises."),
        ActionsJobMessage::ShowMore => text.literal("Afficher la suite de cette étape"),
        ActionsJobMessage::Or => text.literal(" ou"),
        ActionsJobMessage::DownloadAll => text.literal("télécharger le journal brut pour tout voir."),
        ActionsJobMessage::Truncated => text.literal(
            "Le job a produit plus que ce que l’instance conserve\u{202f}; les lignes suivantes ont été supprimées.",
        ),
        ActionsJobMessage::StepNotStarted => text.literal("Cette étape n’a pas démarré."),
        ActionsJobMessage::SetUpJob => text.literal("Préparer le job"),
        ActionsJobMessage::CompleteJob => text.literal("Terminer le job"),
        ActionsJobMessage::Success => text.literal("Réussite"),
        ActionsJobMessage::Failure => text.literal("Échec"),
        ActionsJobMessage::Cancelled => text.literal("Annulé"),
        ActionsJobMessage::Skipped => text.literal("Ignoré"),
        ActionsJobMessage::Queued => text.literal("En file d’attente"),
        ActionsJobMessage::Running => text.literal("En cours"),
        ActionsJobMessage::Waiting => text.literal("En attente"),
        ActionsJobMessage::Approval => text.literal("En attente d’approbation"),
        ActionsJobMessage::WorkflowError => text.literal("Erreur du workflow"),
        ActionsJobMessage::StepCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " étape",
                _ => " étapes",
            });
        }
    }
}
