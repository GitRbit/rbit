//! Spanish (`es`) entries for `catalog/run_page.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_page::RunPageMessage;
use crate::plural::Plural;

entries! { Es:
    RunPageMessage<'a> => MessageWriter |text| {
        RunPageMessage::ApprovalTitle => text.literal("Esta ejecución espera aprobación"),
        RunPageMessage::ApprovalExplanation => text.literal(
            "Procede de una bifurcación o de alguien sin acceso de escritura. Su flujo de trabajo puede ejecutar \
             código que no has revisado, así que alguien con acceso de escritura debe aprobarla primero. Incluso \
             aprobada, se ejecuta con un token de solo lectura y sin secretos ni variables.",
        ),
        RunPageMessage::TrustAuthor => {
            text.literal("Ejecutar siempre sin aprobación las solicitudes de cambios de este autor");
        }
        RunPageMessage::ApproveAndRun => text.literal("Aprobar y ejecutar"),
        RunPageMessage::UntrustedNote => text.literal(
            "Esta ejecución procede de una bifurcación o de un autor externo: tiene un token de solo lectura y no \
             tiene secretos ni variables.",
        ),
        RunPageMessage::UntrustedApprovedNote { approver } => {
            text.literal(
                "Esta ejecución procede de una bifurcación o de un autor externo: tiene un token de solo lectura, no \
                 tiene secretos ni variables y la aprobó ",
            );
            text.argument(approver);
            text.literal(".");
        }
        RunPageMessage::DeploymentWaiting => text.literal("Pendiente de revisión:"),
        RunPageMessage::DeploymentJobs { jobs, count, environment } => {
            text.argument(jobs);
            text.literal(match text.plural(count) {
                Plural::One => " despliega en ",
                _ => " despliegan en ",
            });
            text.argument(environment);
            text.literal(", cuyos revisores deben aprobarlo primero.");
        }
        RunPageMessage::Comment => text.literal("Comentario"),
        RunPageMessage::Optional => text.literal("(opcional)"),
        RunPageMessage::ApproveAndDeploy => text.literal("Aprobar y desplegar"),
        RunPageMessage::Reject => text.literal("Rechazar"),
        RunPageMessage::Needs { jobs } => {
            text.literal("necesita ");
            text.argument(jobs);
        }
    }
}
