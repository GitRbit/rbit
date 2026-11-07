use super::super::{ActionsMessage, Catalog, LocalizedText};

pub(in crate::catalog) const RERUN_PREFIX: &str = "Repetir ";
pub(in crate::catalog) const SUMMARY_PREFIX: &str = "Resumen de ";

pub(in crate::catalog) fn actions(catalog: Catalog, message: ActionsMessage<'_>) -> LocalizedText {
    let literal = match message {
        ActionsMessage::Actions => "Acciones",
        ActionsMessage::Workflow => "Flujo de trabajo",
        ActionsMessage::Commit => "Commit",
        ActionsMessage::Duration => "Duración",
        ActionsMessage::Attempt => "Intento",
        ActionsMessage::TriggeredBy => "Iniciado por",
        ActionsMessage::Jobs => "Trabajos",
        ActionsMessage::Artifacts => "Artefactos",
        ActionsMessage::RunDetails => "Detalles de la ejecución",
        ActionsMessage::CancelRun => "Cancelar ejecución",
        ActionsMessage::RerunFailed => "Repetir los fallidos",
        ActionsMessage::RerunAll => "Repetir todos",
        ActionsMessage::DownloadTar => "Descargar como tar.gz",
        ActionsMessage::Expires => "vence",
        ActionsMessage::RerunJob { name } => return LocalizedText(format!("{RERUN_PREFIX}{name}").into()),
        ActionsMessage::Summary { job } => return LocalizedText(format!("{SUMMARY_PREFIX}{job}").into()),
    };
    catalog.literal_text(literal)
}
