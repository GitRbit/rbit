//! Brazilian Portuguese (`pt-BR`) entries for `catalog/run_page.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_page::RunPageMessage;
use crate::plural::Plural;

entries! { PtBr:
    RunPageMessage<'a> => MessageWriter |text| {
        RunPageMessage::ApprovalTitle => text.literal("Esta execução aguarda aprovação"),
        RunPageMessage::ApprovalExplanation => text.literal(
            "Ela vem de um fork ou de alguém sem acesso de escrita. O fluxo de trabalho pode executar código que você \
             não revisou, então alguém com acesso de escrita precisa aprová-la primeiro. Mesmo aprovada, ela é \
             executada com um token somente leitura e sem segredos nem variáveis.",
        ),
        RunPageMessage::TrustAuthor => {
            text.literal("Sempre executar os pull requests deste autor sem aprovação");
        }
        RunPageMessage::ApproveAndRun => text.literal("Aprovar e executar"),
        RunPageMessage::UntrustedNote => {
            text.literal(
                "Esta execução vem de um fork ou de um autor externo: ela tem um token somente leitura e não tem \
                 segredos nem variáveis.",
            );
        }
        RunPageMessage::UntrustedApprovedNote { approver } => {
            text.literal(
                "Esta execução vem de um fork ou de um autor externo: ela tem um token somente leitura, não tem \
                 segredos nem variáveis e foi aprovada por ",
            );
            text.argument(approver);
            text.literal(".");
        }
        RunPageMessage::DeploymentWaiting => text.literal("Aguardando revisão:"),
        RunPageMessage::DeploymentJobs { jobs, count, environment } => {
            text.argument(jobs);
            text.literal(match text.plural(count) {
                Plural::One => " faz implantação em ",
                _ => " fazem implantação em ",
            });
            text.argument(environment);
            text.literal(", cujos revisores precisam aprovar primeiro.");
        }
        RunPageMessage::Comment => text.literal("Comentário"),
        RunPageMessage::Optional => text.literal("(opcional)"),
        RunPageMessage::ApproveAndDeploy => text.literal("Aprovar e implantar"),
        RunPageMessage::Reject => text.literal("Rejeitar"),
        RunPageMessage::Needs { jobs } => {
            text.literal("depende de ");
            text.argument(jobs);
        }
    }
}
