//! French (`fr`) entries for `catalog/settings/protection.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::protection::ProtectionSentence;
use crate::catalog::settings::protection::{ProtectionMessage, ProtectionText, RuleBadge};
use crate::plural::Plural;

entries! { Fr:
    ProtectionMessage => MessageWriter |text| {
        ProtectionMessage::TickToConfirm => text.literal("Cochez la case pour confirmer."),
        ProtectionMessage::Settings => text.literal("Paramètres"),
        ProtectionMessage::Title => text.literal("Protection des branches"),
        ProtectionMessage::AddRule => text.literal("Ajouter une règle"),
        ProtectionMessage::Deleted => text.literal("Règle supprimée."),
        ProtectionMessage::Empty => text.literal("Aucune branche protégée"),
        ProtectionMessage::EmptyText => text.literal(
            "Ajoutez une règle pour empêcher qu’une branche comme main subisse un push forcé, soit supprimée ou \
             modifiée sans pull request.",
        ),
        ProtectionMessage::NoMatchYet => text.literal("Ne correspond encore à aucune branche."),
        ProtectionMessage::MatchesLabel => text.literal("Correspondances\u{a0}:"),
        ProtectionMessage::NewRule => text.literal("Nouvelle règle de protection de branche"),
        ProtectionMessage::NewRuleIntro => text.literal(
            "La règle s’applique à toutes les branches correspondant à son motif, actuelles et futures.",
        ),
        ProtectionMessage::Saved => text.literal("Règle enregistrée."),
        ProtectionMessage::Rule => text.literal("Règle"),
        ProtectionMessage::Pattern => text.literal("Motif de nom de branche"),
        ProtectionMessage::PatternPlaceholder => text.literal("main"),
        ProtectionMessage::PatternLead => text.literal("Un nom de branche, ou un motif\u{a0}:"),
        ProtectionMessage::PatternOne => text.literal("correspond à l’intérieur d’une partie du nom,"),
        ProtectionMessage::PatternAcross => text.literal("sur plusieurs parties ("),
        ProtectionMessage::PatternChar => text.literal("), "),
        ProtectionMessage::PatternTail => text.literal("à un seul caractère. Sensible à la casse."),
        ProtectionMessage::Protect => text.literal("Protéger les branches correspondantes"),
        ProtectionMessage::BlockForcePush => text.literal("Bloquer les push forcés"),
        ProtectionMessage::BlockForcePushText => text.literal(
            "Les push ne peuvent qu’ajouter des commits\u{202f}; l’historique ne peut pas être réécrit.",
        ),
        ProtectionMessage::BlockDeletion => text.literal("Bloquer la suppression"),
        ProtectionMessage::BlockDeletionText => text.literal("Les branches correspondantes ne peuvent pas être supprimées."),
        ProtectionMessage::RequirePullRequest => text.literal("Exiger une pull request"),
        ProtectionMessage::RequirePullRequestText => text.literal(
            "Les modifications n’arrivent que par la fusion d’une pull request\u{202f}; les push directs vers une \
             branche existante sont refusés.",
        ),
        ProtectionMessage::RequiredApprovals => text.literal("Approbations requises"),
        ProtectionMessage::DismissStale => text.literal("Rejeter les approbations lorsque de nouveaux commits sont poussés"),
        ProtectionMessage::BlockOnChanges => text.literal("Bloquer la fusion tant que des modifications sont demandées"),
        ProtectionMessage::RestrictPushers => text.literal("Restreindre qui peut pousser"),
        ProtectionMessage::RestrictPeople => text.literal(
            "Seules les personnes ci-dessous peuvent pousser ou fusionner. Les clés de déploiement ne le peuvent jamais.",
        ),
        ProtectionMessage::RestrictPeopleTeams => text.literal(
            "Seules les personnes et les équipes ci-dessous peuvent pousser ou fusionner. Les clés de déploiement ne le \
             peuvent jamais.",
        ),
        ProtectionMessage::People => text.literal("Personnes"),
        ProtectionMessage::PeoplePlaceholder => text.literal("alice bob"),
        ProtectionMessage::PeopleHint => text.literal(
            "Noms d’utilisateur séparés par des espaces ou des virgules. Chacun doit avoir un accès en écriture.",
        ),
        ProtectionMessage::Teams => text.literal("Équipes"),
        ProtectionMessage::NoTeams => text.literal("Cette organisation n’a encore aucune équipe."),
        ProtectionMessage::RequireChecks => text.literal("Exiger la réussite des vérifications de statut"),
        ProtectionMessage::RequireChecksText => text.literal(
            "Les pull requests ne sont fusionnées que lorsque le dernier statut de chaque vérification ci-dessous est \
             un succès sur leur commit de tête.",
        ),
        ProtectionMessage::RequiredChecks => text.literal("Vérifications requises"),
        ProtectionMessage::ChecksPlaceholder => text.literal("ci/build"),
        ProtectionMessage::AdminBypass => text.literal("Autoriser les administrateurs à contourner la règle"),
        ProtectionMessage::AdminBypassText => text.literal(
            "Les administrateurs du dépôt ne sont pas soumis à cette règle. Chaque contournement est consigné dans le \
             journal d’audit.",
        ),
        ProtectionMessage::SaveRule => text.literal("Enregistrer la règle"),
        ProtectionMessage::DangerZone => text.literal("Zone de danger"),
        ProtectionMessage::DeleteTitle => text.literal("Supprimer cette règle"),
        ProtectionMessage::DeleteLead => text.literal("Les branches correspondant à"),
        ProtectionMessage::DeleteTail => text.literal("perdent immédiatement sa protection."),
        ProtectionMessage::DeleteConfirm => text.literal("Je comprends que cette règle sera supprimée"),
        ProtectionMessage::DeleteRule => text.literal("Supprimer la règle"),
        ProtectionMessage::NoMatches => text.literal("Ne correspond encore à aucune branche"),
    }
    ProtectionText<'a> => MessageWriter |text| {
        ProtectionText::MaxRules(max) => {
            text.literal("Ce dépôt a atteint le maximum de ");
            text.number(max);
            text.literal(match text.plural(max) {
                Plural::One => " règle. Supprimez-en une pour en ajouter une autre.",
                _ => " règles. Supprimez-en une pour en ajouter une autre.",
            });
        }
        ProtectionText::ChecksHint(max) => {
            text.literal(
                "Un contexte par ligne, exactement tel que votre CI le signale (sensible à la casse), au plus ",
            );
            text.number(max);
            text.literal(".");
        }
        ProtectionText::Matches(names) => {
            text.literal("Correspond à ");
            text.argument(names);
        }
        ProtectionText::MatchesMore(names, more) => {
            text.literal("Correspond à ");
            text.argument(names);
            text.literal(" et ");
            text.number(more);
            text.literal(match text.plural(more) {
                Plural::One => " autre",
                _ => " autres",
            });
        }
        ProtectionText::ListTitle(repository) => {
            text.literal("Protection des branches · ");
            text.argument(repository);
        }
        ProtectionText::NewTitle(repository) => {
            text.literal("Nouvelle règle de protection de branche · ");
            text.argument(repository);
        }
        ProtectionText::EditTitle(pattern, repository) => {
            text.argument(pattern);
            text.literal(" · Protection des branches · ");
            text.argument(repository);
        }
    }
    RuleBadge => MessageWriter |text| {
        RuleBadge::NoForcePushes => text.literal("Pas de push forcé"),
        RuleBadge::NoDeletion => text.literal("Pas de suppression"),
        RuleBadge::PullRequest => text.literal("Pull request"),
        RuleBadge::Approvals(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " approbation",
                _ => " approbations",
            });
        }
        RuleBadge::StatusChecks => text.literal("Vérifications de statut"),
        RuleBadge::Restricted => text.literal("Restreint"),
        RuleBadge::AdminsBypass => text.literal("Contournement admin"),
    }
    ProtectionSentence => RichWriter |text| {
        ProtectionSentence::Intro => {
            text.literal("Les règles s’appliquent à chaque push vers ");
            text.slot(0);
            text.literal(", en HTTP et en SSH, administrateurs compris, sauf si une règle leur permet de la contourner.");
        }
    }
}
