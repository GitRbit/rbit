//! German (`de`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;
use crate::plural::Plural;

entries! { De:
    RunLayoutMessage<'a> => MessageWriter |text| {
        RunLayoutMessage::Summary => text.literal("Übersicht"),
        RunLayoutMessage::Navigation => text.literal("Navigation der Ausführung"),
        RunLayoutMessage::Usage => text.literal("Nutzung"),
        RunLayoutMessage::WorkflowFile => text.literal("Workflow-Datei"),
        RunLayoutMessage::JobCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Job",
                _ => " Jobs",
            });
        }
        RunLayoutMessage::Graph => text.literal("Job-Graph"),
        RunLayoutMessage::MoreJobs { count } => {
            text.literal("+");
            text.number(count);
            text.literal(" weitere");
        }
        RunLayoutMessage::Annotations => text.literal("Anmerkungen"),
        RunLayoutMessage::Errors { count } => {
            text.number(count);
            text.literal(" Fehler");
        }
        RunLayoutMessage::Warnings { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Warnung",
                _ => " Warnungen",
            });
        }
        RunLayoutMessage::Notices { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Hinweis",
                _ => " Hinweise",
            });
        }
        RunLayoutMessage::ErrorLevel => text.literal("Fehler"),
        RunLayoutMessage::WarningLevel => text.literal("Warnung"),
        RunLayoutMessage::NoticeLevel => text.literal("Hinweis"),
        RunLayoutMessage::StepFailed { step } => {
            text.literal("Der Schritt ");
            text.argument(step);
            text.literal(" ist fehlgeschlagen.");
        }
        RunLayoutMessage::MoreAnnotations { count } => {
            text.literal("Und ");
            text.number(count);
            text.literal(" weitere in den Protokollen der Jobs.");
        }
        RunLayoutMessage::TriggeredVia => text.literal("Ausgelöst durch"),
        RunLayoutMessage::Status => text.literal("Status"),
        RunLayoutMessage::TotalDuration => text.literal("Gesamtdauer"),
        RunLayoutMessage::RerunBy { name } => {
            text.literal("Erneut gestartet von ");
            text.argument(name);
        }
        RunLayoutMessage::Inputs => text.literal("Eingaben"),
        RunLayoutMessage::Waited { time } => {
            text.literal("wartete ");
            text.argument(time);
        }
        RunLayoutMessage::RanOn { runner } => {
            text.literal("auf ");
            text.argument(runner);
        }
    }
}
