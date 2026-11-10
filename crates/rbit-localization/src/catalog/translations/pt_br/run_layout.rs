//! Brazilian Portuguese (`pt-BR`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;
use crate::plural::Plural;

entries! { PtBr:
    RunLayoutMessage<'a> => MessageWriter |text| {
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
        RunLayoutMessage::Annotations => text.literal("Anotações"),
        RunLayoutMessage::Errors { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " erro",
                _ => " erros",
            });
        }
        RunLayoutMessage::Warnings { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " aviso",
                _ => " avisos",
            });
        }
        RunLayoutMessage::Notices { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " notificação",
                _ => " notificações",
            });
        }
        RunLayoutMessage::ErrorLevel => text.literal("Erro"),
        RunLayoutMessage::WarningLevel => text.literal("Aviso"),
        RunLayoutMessage::NoticeLevel => text.literal("Notificação"),
        RunLayoutMessage::StepFailed { step } => {
            text.literal("A etapa ");
            text.argument(step);
            text.literal(" falhou.");
        }
        RunLayoutMessage::MoreAnnotations { count } => {
            text.literal("E mais ");
            text.number(count);
            text.literal(" nos logs dos jobs.");
        }
    }
}
