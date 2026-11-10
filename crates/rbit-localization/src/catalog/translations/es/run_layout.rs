//! Spanish (`es`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;
use crate::plural::Plural;

entries! { Es:
    RunLayoutMessage => MessageWriter |text| {
        RunLayoutMessage::Summary => text.literal("Resumen"),
        RunLayoutMessage::Navigation => text.literal("Navegación de la ejecución"),
        RunLayoutMessage::Usage => text.literal("Uso"),
        RunLayoutMessage::WorkflowFile => text.literal("Archivo del flujo de trabajo"),
        RunLayoutMessage::JobCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " trabajo",
                _ => " trabajos",
            });
        }
        RunLayoutMessage::Graph => text.literal("Gráfico de trabajos"),
        RunLayoutMessage::MoreJobs { count } => {
            text.literal("+");
            text.number(count);
            text.literal(" más");
        }
    }
}
