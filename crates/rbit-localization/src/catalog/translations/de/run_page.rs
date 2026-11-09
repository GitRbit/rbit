//! German (`de`) entries for `catalog/run_page.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_page::RunPageMessage;
use crate::plural::Plural;

entries! { De:
    RunPageMessage<'a> => MessageWriter |text| {
        RunPageMessage::ApprovalTitle => text.literal("Diese Ausführung wartet auf Genehmigung"),
        RunPageMessage::ApprovalExplanation => text.literal(
            "Sie stammt aus einem Fork oder von jemandem ohne Schreibzugriff. Ihr Workflow kann Code ausführen, den \
             Sie nicht geprüft haben, daher muss ihn zuerst jemand mit Schreibzugriff genehmigen. Auch genehmigt \
             läuft sie mit einem schreibgeschützten Token und ohne Secrets oder Variablen.",
        ),
        RunPageMessage::TrustAuthor => {
            text.literal("Pull-Requests dieses Autors immer ohne Genehmigung ausführen");
        }
        RunPageMessage::ApproveAndRun => text.literal("Genehmigen und ausführen"),
        RunPageMessage::UntrustedNote => text.literal(
            "Diese Ausführung stammt aus einem Fork oder von einem externen Autor: Sie hat ein schreibgeschütztes \
             Token und keine Secrets oder Variablen.",
        ),
        RunPageMessage::UntrustedApprovedNote { approver } => {
            text.literal(
                "Diese Ausführung stammt aus einem Fork oder von einem externen Autor: Sie hat ein schreibgeschütztes \
                 Token und keine Secrets oder Variablen und wurde genehmigt von ",
            );
            text.argument(approver);
            text.literal(".");
        }
        RunPageMessage::DeploymentWaiting => text.literal("Wartet auf Review:"),
        RunPageMessage::DeploymentJobs { jobs, count, environment } => {
            text.argument(jobs);
            text.literal(match text.plural(count) {
                Plural::One => " stellt bereit in ",
                _ => " stellen bereit in ",
            });
            text.argument(environment);
            text.literal(", deren Reviewer zuerst genehmigen müssen.");
        }
        RunPageMessage::Comment => text.literal("Kommentar"),
        RunPageMessage::Optional => text.literal("(optional)"),
        RunPageMessage::ApproveAndDeploy => text.literal("Genehmigen und bereitstellen"),
        RunPageMessage::Reject => text.literal("Ablehnen"),
        RunPageMessage::Needs { jobs } => {
            text.literal("benötigt ");
            text.argument(jobs);
        }
    }
}
