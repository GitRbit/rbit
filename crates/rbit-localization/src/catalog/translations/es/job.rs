//! Spanish (`es`) entries for `catalog/job.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::job::ActionsJobMessage;
use crate::plural::Plural;

entries! { Es:
    ActionsJobMessage => MessageWriter |text| {
        ActionsJobMessage::Details => text.literal("Detalles del trabajo"),
        ActionsJobMessage::Steps => text.literal("Pasos y registros"),
        ActionsJobMessage::RunsOn => text.literal("se ejecuta en"),
        ActionsJobMessage::Duration => text.literal("Duración"),
        ActionsJobMessage::RawLog => text.literal("Registro completo"),
        ActionsJobMessage::Rerun => text.literal("Repetir trabajo"),
        ActionsJobMessage::NotStarted => text.literal("Aún no ha comenzado"),
        ActionsJobMessage::NotStartedDescription => text.literal("Los pasos y sus registros aparecerán cuando un agente tome este trabajo."),
        ActionsJobMessage::RetainedWindow => text.literal("Las vistas de los pasos son limitadas. Abre un paso para ver más líneas o descarga el registro completo."),
        ActionsJobMessage::Expired => text.literal("Este registro fue eliminado por la política de retención."),
        ActionsJobMessage::NoOutput => text.literal("Sin salida."),
        ActionsJobMessage::EarlierLines => text.literal("Se omiten las líneas anteriores."),
        ActionsJobMessage::ShowMore => text.literal("Mostrar más de este paso"),
        ActionsJobMessage::Or => text.literal(", o"),
        ActionsJobMessage::DownloadAll => text.literal("descarga el registro completo para verlo todo."),
        ActionsJobMessage::Truncated => text.literal("El trabajo escribió más de lo que conserva la instancia; se descartaron las líneas posteriores."),
        ActionsJobMessage::StepNotStarted => text.literal("Este paso no ha comenzado."),
        ActionsJobMessage::SetUpJob => text.literal("Preparar el trabajo"),
        ActionsJobMessage::CompleteJob => text.literal("Finalizar el trabajo"),
        ActionsJobMessage::Success => text.literal("Correcto"),
        ActionsJobMessage::Failure => text.literal("Fallido"),
        ActionsJobMessage::Cancelled => text.literal("Cancelado"),
        ActionsJobMessage::Skipped => text.literal("Omitido"),
        ActionsJobMessage::Queued => text.literal("En cola"),
        ActionsJobMessage::Running => text.literal("En curso"),
        ActionsJobMessage::Waiting => text.literal("En espera"),
        ActionsJobMessage::Approval => text.literal("Pendiente de aprobación"),
        ActionsJobMessage::WorkflowError => text.literal("Error del flujo"),
        ActionsJobMessage::StepCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " paso",
                _ => " pasos",
            });
        }
    }
}
