//! French (`fr`) entries for `catalog/overview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview::ActionsOverviewMessage;
use crate::plural::Plural;

entries! { Fr:
    ActionsOverviewMessage => MessageWriter |text| {
        ActionsOverviewMessage::Workflows => text.literal("Workflows"),
        ActionsOverviewMessage::AllWorkflows => text.literal("Tous les workflows"),
        ActionsOverviewMessage::AllRuns => text.literal("Toutes les exécutions de workflows"),
        ActionsOverviewMessage::Filters => text.literal("Filtres des exécutions de workflows"),
        ActionsOverviewMessage::Branch => text.literal("Branche"),
        ActionsOverviewMessage::Actor => text.literal("Déclencheur"),
        ActionsOverviewMessage::Event => text.literal("Événement"),
        ActionsOverviewMessage::Status => text.literal("Statut"),
        ActionsOverviewMessage::AnyEvent => text.literal("Tous les événements"),
        ActionsOverviewMessage::AnyStatus => text.literal("Tous les statuts"),
        ActionsOverviewMessage::Filter => text.literal("Filtrer"),
        ActionsOverviewMessage::Clear => text.literal("Effacer"),
        ActionsOverviewMessage::EnableWorkflow => text.literal("Activer le workflow"),
        ActionsOverviewMessage::DisableWorkflow => text.literal("Désactiver le workflow"),
        ActionsOverviewMessage::WorkflowDisabled => text.literal(
            "Ce workflow est désactivé\u{a0}: il ne lance aucune exécution, quel que soit l’événement, tant qu’il \
             n’est pas activé.",
        ),
        ActionsOverviewMessage::NoMatchingRuns => text.literal("Aucune exécution correspondante"),
        ActionsOverviewMessage::NoMatchingRunsDescription => {
            text.literal("Aucune exécution de workflow ne correspond à ces filtres.");
        }
        ActionsOverviewMessage::NoRunsYet => text.literal("Aucune exécution pour l’instant"),
        ActionsOverviewMessage::DispatchEmptyDescription => text.literal(
            "Ce workflow s’exécute lorsqu’on le lance\u{a0}: utilisez «\u{a0}Exécuter le workflow\u{a0}» ci-dessus.",
        ),
        ActionsOverviewMessage::NoWorkflowRunsYet => text.literal("Aucune exécution de workflow pour l’instant"),
        ActionsOverviewMessage::WorkflowEmptyDescription => text.literal(
            "Ajoutez un fichier de workflow dans .rbit/workflows et poussez-le\u{a0}: les exécutions apparaîtront ici \
             dès que des runners les prendront en charge.",
        ),
        ActionsOverviewMessage::Pagination => text.literal("Pagination"),
        ActionsOverviewMessage::OlderRuns => text.literal("Exécutions plus anciennes"),
        ActionsOverviewMessage::RunsOnPage { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " exécution sur cette page",
                _ => " exécutions sur cette page",
            });
        }
    }
}
