//! Spanish (`es`) entries for `catalog/overview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview::ActionsOverviewMessage;
use crate::plural::Plural;

entries! { Es:
    ActionsOverviewMessage => MessageWriter |text| {
        ActionsOverviewMessage::Workflows => text.literal("Flujos de trabajo"),
        ActionsOverviewMessage::AllWorkflows => text.literal("Todos los flujos"),
        ActionsOverviewMessage::AllRuns => text.literal("Todas las ejecuciones"),
        ActionsOverviewMessage::Filters => text.literal("Filtros de ejecuciones"),
        ActionsOverviewMessage::Branch => text.literal("Rama"),
        ActionsOverviewMessage::Actor => text.literal("Usuario"),
        ActionsOverviewMessage::Event => text.literal("Evento"),
        ActionsOverviewMessage::Status => text.literal("Estado"),
        ActionsOverviewMessage::AnyEvent => text.literal("Cualquier evento"),
        ActionsOverviewMessage::AnyStatus => text.literal("Cualquier estado"),
        ActionsOverviewMessage::Filter => text.literal("Filtrar"),
        ActionsOverviewMessage::Clear => text.literal("Limpiar"),
        ActionsOverviewMessage::EnableWorkflow => text.literal("Activar flujo"),
        ActionsOverviewMessage::DisableWorkflow => text.literal("Desactivar flujo"),
        ActionsOverviewMessage::WorkflowDisabled => text.literal("Este flujo está desactivado: no inicia ninguna ejecución hasta que se active."),
        ActionsOverviewMessage::NoMatchingRuns => text.literal("No hay ejecuciones coincidentes"),
        ActionsOverviewMessage::NoMatchingRunsDescription => text.literal("Ninguna ejecución coincide con estos filtros."),
        ActionsOverviewMessage::NoRunsYet => text.literal("Aún no hay ejecuciones"),
        ActionsOverviewMessage::DispatchEmptyDescription => text.literal("Este flujo se ejecuta al iniciarlo: usa Ejecutar flujo arriba."),
        ActionsOverviewMessage::NoWorkflowRunsYet => text.literal("Aún no hay ejecuciones de flujos"),
        ActionsOverviewMessage::WorkflowEmptyDescription => text.literal("Añade un archivo en .rbit/workflows y envíalo: las ejecuciones aparecerán cuando las tomen los agentes."),
        ActionsOverviewMessage::Pagination => text.literal("Paginación"),
        ActionsOverviewMessage::OlderRuns => text.literal("Ejecuciones anteriores"),
        ActionsOverviewMessage::RunsOnPage { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " ejecución en esta página",
                _ => " ejecuciones en esta página",
            });
        }
        ActionsOverviewMessage::JobsFinished { done, total } => {
            text.number(done);
            text.literal(" de ");
            text.number(total);
            text.literal(" trabajos terminados");
        }
        ActionsOverviewMessage::JobsRunning { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " trabajo en ejecución",
                _ => " trabajos en ejecución",
            });
        }
        ActionsOverviewMessage::WaitingForRunner => text.literal("Esperando un runner"),
    }
}
