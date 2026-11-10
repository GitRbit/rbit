//! Spanish (`es`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;
use crate::plural::Plural;

entries! { Es:
    RunLayoutMessage<'a> => MessageWriter |text| {
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
        RunLayoutMessage::Annotations => text.literal("Anotaciones"),
        RunLayoutMessage::Errors { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " error",
                _ => " errores",
            });
        }
        RunLayoutMessage::Warnings { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " advertencia",
                _ => " advertencias",
            });
        }
        RunLayoutMessage::Notices { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " aviso",
                _ => " avisos",
            });
        }
        RunLayoutMessage::ErrorLevel => text.literal("Error"),
        RunLayoutMessage::WarningLevel => text.literal("Advertencia"),
        RunLayoutMessage::NoticeLevel => text.literal("Aviso"),
        RunLayoutMessage::StepFailed { step } => {
            text.literal("Falló el paso ");
            text.argument(step);
            text.literal(".");
        }
        RunLayoutMessage::MoreAnnotations { count } => {
            text.literal("Y ");
            text.number(count);
            text.literal(" más en los registros de los trabajos.");
        }
        RunLayoutMessage::TriggeredVia => text.literal("Iniciada por"),
        RunLayoutMessage::Status => text.literal("Estado"),
        RunLayoutMessage::TotalDuration => text.literal("Duración total"),
        RunLayoutMessage::RerunBy { name } => {
            text.literal("Repetida por ");
            text.argument(name);
        }
        RunLayoutMessage::Inputs => text.literal("Entradas"),
        RunLayoutMessage::Waited { time } => {
            text.literal("esperó ");
            text.argument(time);
        }
        RunLayoutMessage::RanOn { runner } => {
            text.literal("en ");
            text.argument(runner);
        }
    }
}
