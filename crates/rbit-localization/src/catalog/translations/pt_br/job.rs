//! Brazilian Portuguese (`pt-BR`) entries for `catalog/job.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::job::ActionsJobMessage;
use crate::plural::Plural;

entries! { PtBr:
    ActionsJobMessage => MessageWriter |text| {
        ActionsJobMessage::Details => text.literal("Detalhes do job"),
        ActionsJobMessage::Steps => text.literal("Etapas e logs"),
        ActionsJobMessage::RunsOn => text.literal("executado em"),
        ActionsJobMessage::Duration => text.literal("Duração"),
        ActionsJobMessage::RawLog => text.literal("Log bruto"),
        ActionsJobMessage::Rerun => text.literal("Executar o job novamente"),
        ActionsJobMessage::NotStarted => text.literal("Ainda não iniciado"),
        ActionsJobMessage::NotStartedDescription => text.literal("As etapas e os logs delas aparecem aqui quando um executor pegar este job."),
        ActionsJobMessage::RetainedWindow => text.literal("As prévias das etapas são limitadas. Abra uma etapa para ver mais linhas ou baixe o log bruto."),
        ActionsJobMessage::Expired => text.literal("Este log foi removido pela retenção de logs."),
        ActionsJobMessage::NoOutput => text.literal("Nenhuma saída."),
        ActionsJobMessage::EarlierLines => text.literal("As linhas anteriores foram omitidas."),
        ActionsJobMessage::ShowMore => text.literal("Mostrar mais desta etapa"),
        ActionsJobMessage::Or => text.literal(", ou"),
        ActionsJobMessage::DownloadAll => text.literal("baixe o log bruto para ver tudo."),
        ActionsJobMessage::Truncated => text.literal("O job escreveu mais do que a instância guarda; as linhas posteriores foram descartadas."),
        ActionsJobMessage::StepNotStarted => text.literal("Esta etapa não foi iniciada."),
        ActionsJobMessage::SetUpJob => text.literal("Preparar o trabalho"),
        ActionsJobMessage::CompleteJob => text.literal("Concluir o trabalho"),
        ActionsJobMessage::Success => text.literal("Sucesso"),
        ActionsJobMessage::Failure => text.literal("Falha"),
        ActionsJobMessage::Cancelled => text.literal("Cancelado"),
        ActionsJobMessage::Skipped => text.literal("Ignorado"),
        ActionsJobMessage::Queued => text.literal("Na fila"),
        ActionsJobMessage::Running => text.literal("Em andamento"),
        ActionsJobMessage::Waiting => text.literal("Aguardando"),
        ActionsJobMessage::Approval => text.literal("Aguardando aprovação"),
        ActionsJobMessage::WorkflowError => text.literal("Erro no fluxo de trabalho"),
        ActionsJobMessage::StepCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " etapa",
                _ => " etapas",
            });
        }
    }
}
