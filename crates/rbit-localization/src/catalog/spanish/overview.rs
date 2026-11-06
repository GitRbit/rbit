use super::super::{ActionsOverviewMessage as Message, Catalog, LocalizedText};

pub(in crate::catalog) fn overview(catalog: Catalog, message: Message) -> LocalizedText {
    let literal = match message {
        Message::Workflows => "Flujos de trabajo",
        Message::AllWorkflows => "Todos los flujos",
        Message::AllRuns => "Todas las ejecuciones",
        Message::Filters => "Filtros de ejecuciones",
        Message::Branch => "Rama",
        Message::Actor => "Usuario",
        Message::Event => "Evento",
        Message::Status => "Estado",
        Message::AnyEvent => "Cualquier evento",
        Message::AnyStatus => "Cualquier estado",
        Message::Filter => "Filtrar",
        Message::Clear => "Limpiar",
        Message::EnableWorkflow => "Activar flujo",
        Message::DisableWorkflow => "Desactivar flujo",
        Message::WorkflowDisabled => "Este flujo está desactivado: no inicia ninguna ejecución hasta que se active.",
        Message::NoMatchingRuns => "No hay ejecuciones coincidentes",
        Message::NoMatchingRunsDescription => "Ninguna ejecución coincide con estos filtros.",
        Message::NoRunsYet => "Aún no hay ejecuciones",
        Message::DispatchEmptyDescription => "Este flujo se ejecuta al iniciarlo: usa Ejecutar flujo arriba.",
        Message::NoWorkflowRunsYet => "Aún no hay ejecuciones de flujos",
        Message::WorkflowEmptyDescription => {
            "Añade un archivo en .rbit/workflows y envíalo: las ejecuciones aparecerán cuando las tomen los agentes."
        }
        Message::Pagination => "Paginación",
        Message::OlderRuns => "Ejecuciones anteriores",
        Message::RunsOnPage { count } => {
            return LocalizedText(format!("{count} {} en esta página", if count == 1 { "ejecución" } else { "ejecuciones" }).into());
        }
    };
    catalog.literal_text(literal)
}
