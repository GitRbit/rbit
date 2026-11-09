//! French (`fr`) entries for `catalog/run_page.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_page::RunPageMessage;
use crate::plural::Plural;

entries! { Fr:
    RunPageMessage<'a> => MessageWriter |text| {
        RunPageMessage::ApprovalTitle => text.literal("Cette exécution attend une approbation"),
        RunPageMessage::ApprovalExplanation => text.literal(
            "Elle provient d’un fork ou d’une personne sans accès en écriture. Son workflow peut exécuter du code que \
             vous n’avez pas relu\u{a0}: une personne ayant accès en écriture doit donc d’abord l’approuver. Même \
             approuvée, elle s’exécute avec un jeton en lecture seule et sans secrets ni variables.",
        ),
        RunPageMessage::TrustAuthor => text.literal("Toujours exécuter les pull requests de cet auteur sans approbation"),
        RunPageMessage::ApproveAndRun => text.literal("Approuver et exécuter"),
        RunPageMessage::UntrustedNote => text.literal(
            "Cette exécution provient d’un fork ou d’un auteur externe\u{a0}: elle dispose d’un jeton en lecture seule, \
             sans secrets ni variables.",
        ),
        RunPageMessage::UntrustedApprovedNote { approver } => {
            text.literal(
                "Cette exécution provient d’un fork ou d’un auteur externe\u{a0}: elle dispose d’un jeton en lecture \
                 seule, sans secrets ni variables, et a été approuvée par ",
            );
            text.argument(approver);
            text.literal(".");
        }
        RunPageMessage::DeploymentWaiting => text.literal("En attente de revue\u{a0}:"),
        RunPageMessage::DeploymentJobs { jobs, count, environment } => {
            text.argument(jobs);
            text.literal(match text.plural(count) {
                Plural::One => " effectue un déploiement vers ",
                _ => " effectuent un déploiement vers ",
            });
            text.argument(environment);
            text.literal(match text.plural(count) {
                Plural::One => ", dont les relecteurs doivent d’abord l’approuver.",
                _ => ", dont les relecteurs doivent d’abord les approuver.",
            });
        }
        RunPageMessage::Comment => text.literal("Commentaire"),
        RunPageMessage::Optional => text.literal("(facultatif)"),
        RunPageMessage::ApproveAndDeploy => text.literal("Approuver et déployer"),
        RunPageMessage::Reject => text.literal("Refuser"),
        RunPageMessage::Needs { jobs } => {
            text.literal("dépend de ");
            text.argument(jobs);
        }
    }
}
