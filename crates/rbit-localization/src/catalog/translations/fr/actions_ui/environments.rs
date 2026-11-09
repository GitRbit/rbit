//! French (`fr`) entries for `catalog/actions_ui/environments.rs`.

use crate::catalog::actions_ui::environments::ActionsEnvironmentMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Fr:
    ActionsEnvironmentMessage<'a> => MessageWriter |text| {
        ActionsEnvironmentMessage::Title => text.literal("Environnements"),
        ActionsEnvironmentMessage::Intro { key } => {
            text.literal("Un job qui désigne un environnement avec ");
            text.argument(key);
            text.literal(
                " reçoit ses secrets et ses variables, attend l’un de ses relecteurs s’il en a, et ne s’exécute que \
                 depuis les branches qu’il autorise. Un environnement apparaît ici la première fois qu’un workflow le \
                 désigne.",
            );
        }
        ActionsEnvironmentMessage::NoEnvironments => text.literal("Aucun environnement"),
        ActionsEnvironmentMessage::NoEnvironmentsHint { key } => {
            text.literal("Ajoutez-en un ci-dessous, ou exécutez un workflow dont un job en désigne un avec ");
            text.argument(key);
            text.literal(".");
        }
        ActionsEnvironmentMessage::NewEnvironment => text.literal("Nouvel environnement"),
        ActionsEnvironmentMessage::Name => text.literal("Nom"),
        ActionsEnvironmentMessage::NamePlaceholder => text.literal("production"),
        ActionsEnvironmentMessage::AddEnvironment => text.literal("Ajouter l’environnement"),
        ActionsEnvironmentMessage::Reviewers { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " relecteur",
                _ => " relecteurs",
            });
        }
        ActionsEnvironmentMessage::ProtectedBranches => text.literal("branches protégées"),
        ActionsEnvironmentMessage::Branches { patterns } => {
            text.literal("branches ");
            text.argument(patterns);
        }
        ActionsEnvironmentMessage::NoProtection => text.literal("Aucune protection"),
        ActionsEnvironmentMessage::ProtectionRules => text.literal("Règles de protection"),
        ActionsEnvironmentMessage::RequiredReviewers => text.literal("Relecteurs requis"),
        ActionsEnvironmentMessage::ReviewersPlaceholder => text.literal("un nom d’utilisateur par ligne"),
        ActionsEnvironmentMessage::ReviewersHint => text.literal(
            "Jusqu’à six personnes pouvant lire ce dépôt. Un job dans cet environnement attend que l’une d’elles \
             l’approuve\u{202f}; un job refusé échoue. Sans relecteur, les jobs s’exécutent sans revue.",
        ),
        ActionsEnvironmentMessage::PreventSelfReview => text.literal("Interdire l’auto-approbation"),
        ActionsEnvironmentMessage::PreventSelfReviewHint => text.literal(
            "La personne qui a lancé une exécution ne peut pas approuver ses déploiements.",
        ),
        ActionsEnvironmentMessage::WaitTimer => text.literal("Délai d’attente"),
        ActionsEnvironmentMessage::WaitTimerHint => text.literal(
            "Nombre de minutes pendant lesquelles un job de cet environnement attend, une fois prêt et approuvé, avant \
             de s’exécuter. 0 pour aucune attente\u{202f}; au plus 43200 (30\u{a0}jours).",
        ),
        ActionsEnvironmentMessage::DeploymentBranches => text.literal("Branches et tags de déploiement"),
        ActionsEnvironmentMessage::AnyBranch => text.literal("N’importe quelle branche ou n’importe quel tag"),
        ActionsEnvironmentMessage::ProtectedOnly => text.literal("Branches protégées uniquement"),
        ActionsEnvironmentMessage::ProtectedOnlyHint => text.literal(
            "Les branches soumises à une règle de protection de branche.",
        ),
        ActionsEnvironmentMessage::SelectedBranches => text.literal("Branches et tags sélectionnés"),
        ActionsEnvironmentMessage::PatternsLabel => text.literal("Motifs de branches et de tags"),
        ActionsEnvironmentMessage::PatternsPlaceholder => text.literal("main"),
        ActionsEnvironmentMessage::PatternsHint { branch, tags, tag, exclude } => {
            text.literal("Un motif par ligne, comme dans les filtres de workflow. Les branches par leur nom (");
            text.argument(branch);
            text.literal("), les tags avec ");
            text.argument(tags);
            text.literal(" (");
            text.argument(tag);
            text.literal(")\u{202f}; ");
            text.argument(exclude);
            text.literal(" exclut.");
        }
        ActionsEnvironmentMessage::SaveRules => text.literal("Enregistrer les règles de protection"),
        ActionsEnvironmentMessage::DeleteTitle => text.literal("Supprimer cet environnement"),
        ActionsEnvironmentMessage::DeleteHint => text.literal(
            "Ses secrets, ses variables et ses règles sont également supprimés. Les jobs qui attendent ses relecteurs \
             s’exécutent immédiatement.",
        ),
        ActionsEnvironmentMessage::DeleteEnvironment => text.literal("Supprimer l’environnement"),
    }
}
