//! German (`de`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;
use crate::plural::Plural;

entries! { De:
    RunLayoutMessage => MessageWriter |text| {
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
    }
}
