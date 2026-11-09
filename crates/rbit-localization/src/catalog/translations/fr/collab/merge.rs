//! French (`fr`) entries for `catalog/collab/merge.rs`.

use crate::catalog::collab::merge::MergeMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Fr:
    MergeMessage<'a> => MessageWriter |text| {
        MergeMessage::Merge => text.literal("Fusion"),
        MergeMessage::ConflictingFiles => text.literal("Fichiers en conflit"),
        MergeMessage::WorkInProgress => text.literal("Cette pull request est encore en cours d’élaboration."),
        MergeMessage::ReadyForReview => text.literal("Prête pour la revue"),
        MergeMessage::MergeMethod => text.literal("Méthode de fusion"),
        MergeMessage::CreateMergeCommit => text.literal("Créer un commit de fusion"),
        MergeMessage::SquashAndMerge => text.literal("Fusionner par squash"),
        MergeMessage::RebaseAndMerge => text.literal("Rebaser et fusionner"),
        MergeMessage::EditMessage => text.literal("Modifier le message de commit"),
        MergeMessage::Message => text.literal("Message"),
        MergeMessage::TitlePlaceholder { title, squash } => {
            text.argument(title);
            text.literal(" — ou, avec un squash, ");
            text.argument(squash);
        }
        MergeMessage::DeleteHead => text.literal("Supprimer la branche source après la fusion"),
        MergeMessage::Bypass => text.literal(
            "Fusionner sans satisfaire les exigences de protection de branche (cette action est journalisée)",
        ),
        MergeMessage::MergePull => text.literal("Fusionner la pull request"),
        MergeMessage::WriteAccessOnly => text.literal(
            "Seules les personnes disposant d’un accès en écriture à ce dépôt peuvent fusionner.",
        ),
        MergeMessage::PullOpen => text.literal("La pull request est ouverte"),
        MergeMessage::PullClosed => text.literal("La pull request est fermée"),
        MergeMessage::StillDraft => text.literal("Cette pull request est encore un brouillon"),
        MergeMessage::HeadExists => text.literal("La branche source existe"),
        MergeMessage::HeadDeleted => text.literal("La branche source a été supprimée"),
        MergeMessage::NoConflicts => text.literal("Aucun conflit avec la branche de base"),
        MergeMessage::Conflicts => text.literal("Cette branche a des conflits à résoudre"),
        MergeMessage::CheckingConflicts => text.literal("Recherche de conflits…"),
        MergeMessage::MergeabilityError => text.literal("Impossible de vérifier si la fusion est possible"),
        MergeMessage::Approvals { have, need } => {
            text.number(have);
            text.literal(" sur ");
            text.number(need);
            text.literal(match text.plural(need) {
                Plural::One => " approbation requise",
                _ => " approbations requises",
            });
        }
        MergeMessage::NoChangesRequested => text.literal("Aucune modification demandée"),
        MergeMessage::ChangesRequestedBy { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " relecteur a demandé des modifications",
                _ => " relecteurs ont demandé des modifications",
            });
        }
        MergeMessage::MayPush => text.literal("Vous pouvez pousser vers la branche de base"),
        MergeMessage::MayNotPush => text.literal("Vous n’êtes pas autorisé à pousser vers la branche de base"),
        MergeMessage::NoChecks => text.literal("Aucune vérification de statut"),
        MergeMessage::ChecksPending => text.literal("Certaines vérifications ne sont pas encore terminées"),
        MergeMessage::ChecksPassed => text.literal("Toutes les vérifications ont réussi"),
        MergeMessage::ChecksFailed => text.literal("Certaines vérifications ont échoué"),
        MergeMessage::RequiredPassed { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " vérification requise a réussi",
                _ => " vérifications requises ont réussi",
            });
        }
        MergeMessage::RequiredFailed { failing, required } => {
            text.number(failing);
            text.literal(" sur ");
            text.number(required);
            text.literal(match text.plural(failing) {
                Plural::One => " vérification requise a échoué",
                _ => " vérifications requises ont échoué",
            });
        }
        MergeMessage::RequiredWaiting { waiting, required } => {
            text.literal("En attente de ");
            text.number(waiting);
            text.literal(" sur ");
            text.number(required);
            text.literal(match text.plural(required) {
                Plural::One => " vérification requise",
                _ => " vérifications requises",
            });
        }
    }
}
