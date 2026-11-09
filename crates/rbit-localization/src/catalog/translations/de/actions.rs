//! German (`de`) entries for `catalog/actions.rs`.

use crate::catalog::actions::ActionsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { De:
    ActionsMessage<'a> => MessageWriter |text| {
        ActionsMessage::Actions => text.literal("Actions"),
        ActionsMessage::Workflow => text.literal("Workflow"),
        ActionsMessage::Commit => text.literal("Commit"),
        ActionsMessage::Duration => text.literal("Dauer"),
        ActionsMessage::Attempt => text.literal("Versuch"),
        ActionsMessage::TriggeredBy => text.literal("Ausgelöst von"),
        ActionsMessage::Jobs => text.literal("Jobs"),
        ActionsMessage::Artifacts => text.literal("Artefakte"),
        ActionsMessage::RunDetails => text.literal("Details der Ausführung"),
        ActionsMessage::CancelRun => text.literal("Ausführung abbrechen"),
        ActionsMessage::RerunFailed => text.literal("Fehlgeschlagene erneut ausführen"),
        ActionsMessage::RerunAll => text.literal("Alle erneut ausführen"),
        ActionsMessage::DownloadTar => text.literal("Als tar.gz herunterladen"),
        ActionsMessage::Expires => text.literal("läuft ab"),
        ActionsMessage::RerunJob { name } => {
            text.argument(name);
            text.literal(" erneut ausführen");
        }
        ActionsMessage::Summary { job } => {
            text.literal("Zusammenfassung von ");
            text.argument(job);
        }
    }
}
