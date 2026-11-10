//! Russian (`ru`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;
use crate::plural::Plural;

entries! { Ru:
    RunLayoutMessage => MessageWriter |text| {
        RunLayoutMessage::Summary => text.literal("Сводка"),
        RunLayoutMessage::Navigation => text.literal("Навигация по запуску"),
        RunLayoutMessage::Usage => text.literal("Использование"),
        RunLayoutMessage::WorkflowFile => text.literal("Файл рабочего процесса"),
        RunLayoutMessage::JobCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " задание",
                Plural::Few => " задания",
                _ => " заданий",
            });
        }
    }
}
