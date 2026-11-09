//! German (`de`) entries for `catalog/job.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::job::ActionsJobMessage;
use crate::plural::Plural;

entries! { De:
    ActionsJobMessage => MessageWriter |text| {
        ActionsJobMessage::Details => text.literal("Job-Details"),
        ActionsJobMessage::Steps => text.literal("Schritte und Protokolle"),
        ActionsJobMessage::RunsOn => text.literal("läuft auf"),
        ActionsJobMessage::Duration => text.literal("Dauer"),
        ActionsJobMessage::RawLog => text.literal("Rohprotokoll"),
        ActionsJobMessage::Rerun => text.literal("Job erneut ausführen"),
        ActionsJobMessage::NotStarted => text.literal("Noch nicht gestartet"),
        ActionsJobMessage::NotStartedDescription => text.literal(
            "Die Schritte und ihre Protokolle erscheinen hier, sobald ein Runner diesen Job übernimmt.",
        ),
        ActionsJobMessage::RetainedWindow => text.literal(
            "Schrittvorschauen sind begrenzt. Öffnen Sie einen Schritt für weitere Zeilen oder laden Sie das \
             Rohprotokoll herunter.",
        ),
        ActionsJobMessage::Expired => text.literal("Dieses Protokoll wurde durch die Protokollaufbewahrung entfernt."),
        ActionsJobMessage::NoOutput => text.literal("Keine Ausgabe."),
        ActionsJobMessage::EarlierLines => text.literal("Frühere Zeilen werden ausgelassen."),
        ActionsJobMessage::ShowMore => text.literal("Mehr von diesem Schritt anzeigen"),
        ActionsJobMessage::Or => text.literal(" oder"),
        ActionsJobMessage::DownloadAll => text.literal("das Rohprotokoll herunterladen, um alles zu sehen."),
        ActionsJobMessage::Truncated => text.literal(
            "Der Job hat mehr ausgegeben, als die Instanz aufbewahrt; spätere Zeilen wurden verworfen.",
        ),
        ActionsJobMessage::StepNotStarted => text.literal("Dieser Schritt wurde noch nicht gestartet."),
        ActionsJobMessage::Success => text.literal("Erfolgreich"),
        ActionsJobMessage::Failure => text.literal("Fehlgeschlagen"),
        ActionsJobMessage::Cancelled => text.literal("Abgebrochen"),
        ActionsJobMessage::Skipped => text.literal("Übersprungen"),
        ActionsJobMessage::Queued => text.literal("In der Warteschlange"),
        ActionsJobMessage::Running => text.literal("Läuft"),
        ActionsJobMessage::Waiting => text.literal("Wartet"),
        ActionsJobMessage::Approval => text.literal("Wartet auf Genehmigung"),
        ActionsJobMessage::WorkflowError => text.literal("Workflow-Fehler"),
        ActionsJobMessage::StepCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Schritt",
                _ => " Schritte",
            });
        }
    }
}
