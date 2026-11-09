//! German (`de`) entries for `catalog/overview_controls.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview_controls::OverviewControlMessage;

entries! { De:
    OverviewControlMessage => MessageWriter |text| {
        OverviewControlMessage::Push => text.literal("Push"),
        OverviewControlMessage::PullRequest => text.literal("Pull-Request"),
        OverviewControlMessage::Manual => text.literal("manuell"),
        OverviewControlMessage::Schedule => text.literal("Zeitplan"),
        OverviewControlMessage::WorkflowRun => text.literal("nach einem Workflow"),
        OverviewControlMessage::Dispatch => text.literal("Dispatch"),
        OverviewControlMessage::Success => text.literal("Erfolgreich"),
        OverviewControlMessage::Failure => text.literal("Fehlgeschlagen"),
        OverviewControlMessage::Cancelled => text.literal("Abgebrochen"),
        OverviewControlMessage::Skipped => text.literal("Übersprungen"),
        OverviewControlMessage::Queued => text.literal("In der Warteschlange"),
        OverviewControlMessage::Running => text.literal("Läuft"),
        OverviewControlMessage::Waiting => text.literal("Wartet"),
        OverviewControlMessage::Approval => text.literal("Wartet auf Genehmigung"),
        OverviewControlMessage::RunWorkflow => text.literal("Workflow ausführen"),
        OverviewControlMessage::WorkflowFrom => text.literal("Workflow verwenden aus"),
        OverviewControlMessage::Change => text.literal("Ändern"),
    }
}
