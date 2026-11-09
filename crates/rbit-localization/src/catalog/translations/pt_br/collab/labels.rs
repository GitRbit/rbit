//! Brazilian Portuguese (`pt-BR`) entries for `catalog/collab/labels.rs`.

use crate::catalog::collab::labels::LabelMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { PtBr:
    LabelMessage<'a> => MessageWriter |text| {
        LabelMessage::NewLabel => text.literal("Nova etiqueta"),
        LabelMessage::Name => text.literal("Nome"),
        LabelMessage::Colour => text.literal("Cor"),
        LabelMessage::CreateLabel => text.literal("Criar etiqueta"),
        LabelMessage::Count { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " etiqueta",
                _ => " etiquetas",
            });
        }
        LabelMessage::EmptyTitle => text.literal("Ainda não há etiquetas"),
        LabelMessage::EmptyText => {
            text.literal("As etiquetas organizam issues e pull requests: bug, melhoria, boa primeira issue…");
        }
        LabelMessage::CreateDefaults => text.literal("Criar etiquetas padrão"),
        LabelMessage::OpenIssues { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " issue aberta",
                _ => " issues abertas",
            });
        }
        LabelMessage::DeleteConfirm { name } => {
            text.literal("Excluir a etiqueta “");
            text.argument(name);
            text.literal("”? Ela será removida de todas as issues.");
        }
    }
}
