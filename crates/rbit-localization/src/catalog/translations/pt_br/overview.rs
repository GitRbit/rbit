//! Brazilian Portuguese (`pt-BR`) entries for `catalog/overview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview::ActionsOverviewMessage;
use crate::plural::Plural;

entries! { PtBr:
    ActionsOverviewMessage => MessageWriter |text| {
        ActionsOverviewMessage::Workflows => text.literal("Fluxos de trabalho"),
        ActionsOverviewMessage::AllWorkflows => text.literal("Todos os fluxos de trabalho"),
        ActionsOverviewMessage::AllRuns => text.literal("Todas as execuções"),
        ActionsOverviewMessage::Filters => text.literal("Filtros de execuções"),
        ActionsOverviewMessage::Branch => text.literal("Branch"),
        ActionsOverviewMessage::Actor => text.literal("Usuário"),
        ActionsOverviewMessage::Event => text.literal("Evento"),
        ActionsOverviewMessage::Status => text.literal("Status"),
        ActionsOverviewMessage::AnyEvent => text.literal("Qualquer evento"),
        ActionsOverviewMessage::AnyStatus => text.literal("Qualquer status"),
        ActionsOverviewMessage::Filter => text.literal("Filtrar"),
        ActionsOverviewMessage::Clear => text.literal("Limpar"),
        ActionsOverviewMessage::EnableWorkflow => text.literal("Ativar fluxo de trabalho"),
        ActionsOverviewMessage::DisableWorkflow => text.literal("Desativar fluxo de trabalho"),
        ActionsOverviewMessage::WorkflowDisabled => text.literal(
            "Este fluxo de trabalho está desativado: ele não inicia nenhuma execução, para nenhum evento, até ser ativado.",
        ),
        ActionsOverviewMessage::NoMatchingRuns => text.literal("Nenhuma execução correspondente"),
        ActionsOverviewMessage::NoMatchingRunsDescription => text.literal("Nenhuma execução corresponde a estes filtros."),
        ActionsOverviewMessage::NoRunsYet => text.literal("Nenhuma execução ainda"),
        ActionsOverviewMessage::DispatchEmptyDescription => text.literal(
            "Este fluxo de trabalho é executado quando é iniciado: use Executar fluxo de trabalho acima.",
        ),
        ActionsOverviewMessage::NoWorkflowRunsYet => text.literal("Nenhuma execução de fluxo de trabalho ainda"),
        ActionsOverviewMessage::WorkflowEmptyDescription => text.literal(
            "Adicione um arquivo de fluxo de trabalho em .rbit/workflows e faça push dele: as execuções aparecem aqui \
             conforme os executores as pegam.",
        ),
        ActionsOverviewMessage::Pagination => text.literal("Paginação"),
        ActionsOverviewMessage::OlderRuns => text.literal("Execuções anteriores"),
        ActionsOverviewMessage::RunsOnPage { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " execução nesta página",
                _ => " execuções nesta página",
            });
        }
    }
}
