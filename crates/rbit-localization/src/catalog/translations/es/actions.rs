//! Spanish (`es`) entries for `catalog/actions.rs`.

use crate::catalog::actions::ActionsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Es:
    ActionsMessage<'a> => MessageWriter |text| {
        ActionsMessage::Actions => text.literal("Acciones"),
        ActionsMessage::Workflow => text.literal("Flujo de trabajo"),
        ActionsMessage::Commit => text.literal("Commit"),
        ActionsMessage::Duration => text.literal("Duración"),
        ActionsMessage::Attempt => text.literal("Intento"),
        ActionsMessage::TriggeredBy => text.literal("Iniciado por"),
        ActionsMessage::Jobs => text.literal("Trabajos"),
        ActionsMessage::Artifacts => text.literal("Artefactos"),
        ActionsMessage::RunDetails => text.literal("Detalles de la ejecución"),
        ActionsMessage::CancelRun => text.literal("Cancelar ejecución"),
        ActionsMessage::RerunFailed => text.literal("Repetir los fallidos"),
        ActionsMessage::RerunAll => text.literal("Repetir todos"),
        ActionsMessage::DownloadTar => text.literal("Descargar como tar.gz"),
        ActionsMessage::Expires => text.literal("vence"),
        ActionsMessage::RerunJob { name } => {
            text.literal("Repetir ");
            text.argument(name);
        }
        ActionsMessage::Summary { job } => {
            text.literal("Resumen de ");
            text.argument(job);
        }
    }
}
