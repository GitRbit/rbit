//! French (`fr`) entries for `catalog/actions.rs`.

use crate::catalog::actions::ActionsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Fr:
    ActionsMessage<'a> => MessageWriter |text| {
        ActionsMessage::Actions => text.literal("Actions"),
        ActionsMessage::Workflow => text.literal("Workflow"),
        ActionsMessage::Commit => text.literal("Commit"),
        ActionsMessage::Duration => text.literal("Durée"),
        ActionsMessage::Attempt => text.literal("Tentative"),
        ActionsMessage::TriggeredBy => text.literal("Déclenchée par"),
        ActionsMessage::Jobs => text.literal("Jobs"),
        ActionsMessage::Artifacts => text.literal("Artefacts"),
        ActionsMessage::RunDetails => text.literal("Détails de l’exécution"),
        ActionsMessage::CancelRun => text.literal("Annuler l’exécution"),
        ActionsMessage::RerunFailed => text.literal("Relancer les jobs en échec"),
        ActionsMessage::RerunAll => text.literal("Tout relancer"),
        ActionsMessage::DownloadTar => text.literal("Télécharger en tar.gz"),
        ActionsMessage::Expires => text.literal("expire"),
        ActionsMessage::RerunJob { name } => {
            text.literal("Relancer ");
            text.argument(name);
        }
        ActionsMessage::Summary { job } => {
            text.literal("Résumé de ");
            text.argument(job);
        }
    }
}
