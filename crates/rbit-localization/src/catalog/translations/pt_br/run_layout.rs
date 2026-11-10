//! Brazilian Portuguese (`pt-BR`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;
use crate::plural::Plural;

entries! { PtBr:
    RunLayoutMessage => MessageWriter |text| {
        RunLayoutMessage::Summary => text.literal("Resumo"),
        RunLayoutMessage::Navigation => text.literal("Navegação da execução"),
        RunLayoutMessage::Usage => text.literal("Uso"),
        RunLayoutMessage::WorkflowFile => text.literal("Arquivo do fluxo de trabalho"),
        RunLayoutMessage::JobCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " job",
                _ => " jobs",
            });
        }
        RunLayoutMessage::Graph => text.literal("Grafo de jobs"),
        RunLayoutMessage::MoreJobs { count } => {
            text.literal("+");
            text.number(count);
            text.literal(" a mais");
        }
    }
}
