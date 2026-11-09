//! Brazilian Portuguese (`pt-BR`) entries for `catalog/actions.rs`.

use crate::catalog::actions::ActionsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { PtBr:
    ActionsMessage<'a> => MessageWriter |text| {
        ActionsMessage::Actions => text.literal("Actions"),
        ActionsMessage::Workflow => text.literal("Fluxo de trabalho"),
        ActionsMessage::Commit => text.literal("Commit"),
        ActionsMessage::Duration => text.literal("Duração"),
        ActionsMessage::Attempt => text.literal("Tentativa"),
        ActionsMessage::TriggeredBy => text.literal("Acionado por"),
        ActionsMessage::Jobs => text.literal("Jobs"),
        ActionsMessage::Artifacts => text.literal("Artefatos"),
        ActionsMessage::RunDetails => text.literal("Detalhes da execução"),
        ActionsMessage::CancelRun => text.literal("Cancelar execução"),
        ActionsMessage::RerunFailed => text.literal("Executar novamente os que falharam"),
        ActionsMessage::RerunAll => text.literal("Executar todos novamente"),
        ActionsMessage::DownloadTar => text.literal("Baixar como tar.gz"),
        ActionsMessage::Expires => text.literal("expira"),
        ActionsMessage::RerunJob { name } => {
            text.literal("Executar novamente ");
            text.argument(name);
        }
        ActionsMessage::Summary { job } => {
            text.literal("Resumo de ");
            text.argument(job);
        }
    }
}
