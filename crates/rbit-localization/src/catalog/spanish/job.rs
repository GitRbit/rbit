use super::super::{ActionsJobMessage as Message, Catalog, LocalizedText};

pub(in crate::catalog) fn job(catalog: Catalog, message: Message) -> LocalizedText {
    let literal = match message {
        Message::Details => "Detalles del trabajo",
        Message::Steps => "Pasos y registros",
        Message::RunsOn => "se ejecuta en",
        Message::Duration => "Duración",
        Message::RawLog => "Registro completo",
        Message::Rerun => "Repetir trabajo",
        Message::NotStarted => "Aún no ha comenzado",
        Message::NotStartedDescription => "Los pasos y sus registros aparecerán cuando un agente tome este trabajo.",
        Message::RetainedWindow => {
            "Las vistas de los pasos son limitadas. Abre un paso para ver más líneas o descarga el registro completo."
        }
        Message::Expired => "Este registro fue eliminado por la política de retención.",
        Message::NoOutput => "Sin salida.",
        Message::EarlierLines => "Se omiten las líneas anteriores.",
        Message::ShowMore => "Mostrar más de este paso",
        Message::Or => ", o",
        Message::DownloadAll => "descarga el registro completo para verlo todo.",
        Message::Truncated => "El trabajo escribió más de lo que conserva la instancia; se descartaron las líneas posteriores.",
        Message::StepNotStarted => "Este paso no ha comenzado.",
        Message::Success => "Correcto",
        Message::Failure => "Fallido",
        Message::Cancelled => "Cancelado",
        Message::Skipped => "Omitido",
        Message::Queued => "En cola",
        Message::Running => "En curso",
        Message::Waiting => "En espera",
        Message::Approval => "Pendiente de aprobación",
        Message::WorkflowError => "Error del flujo",
        Message::StepCount { count } => return LocalizedText(format!("{count} {}", if count == 1 { "paso" } else { "pasos" }).into()),
    };
    catalog.literal_text(literal)
}
