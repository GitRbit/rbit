//! Brazilian Portuguese (`pt-BR`) entries for `catalog/overview_controls.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview_controls::OverviewControlMessage;

entries! { PtBr:
    OverviewControlMessage => MessageWriter |text| {
        OverviewControlMessage::Push => text.literal("push"),
        OverviewControlMessage::PullRequest => text.literal("pull request"),
        OverviewControlMessage::Manual => text.literal("manual"),
        OverviewControlMessage::Schedule => text.literal("agendamento"),
        OverviewControlMessage::WorkflowRun => text.literal("após um fluxo de trabalho"),
        OverviewControlMessage::Dispatch => text.literal("disparo"),
        OverviewControlMessage::Success => text.literal("Sucesso"),
        OverviewControlMessage::Failure => text.literal("Falha"),
        OverviewControlMessage::Cancelled => text.literal("Cancelado"),
        OverviewControlMessage::Skipped => text.literal("Ignorado"),
        OverviewControlMessage::Queued => text.literal("Na fila"),
        OverviewControlMessage::Running => text.literal("Em andamento"),
        OverviewControlMessage::Waiting => text.literal("Aguardando"),
        OverviewControlMessage::Approval => text.literal("Aguardando aprovação"),
        OverviewControlMessage::RunWorkflow => text.literal("Executar fluxo de trabalho"),
        OverviewControlMessage::WorkflowFrom => text.literal("Usar o fluxo de trabalho de"),
        OverviewControlMessage::Change => text.literal("Alterar"),
    }
}
