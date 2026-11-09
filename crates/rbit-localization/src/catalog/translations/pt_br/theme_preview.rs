//! Brazilian Portuguese (`pt-BR`) entries for `catalog/theme_preview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme_preview::ThemePreviewMessage;
use crate::plural::Plural;

entries! { PtBr:
    ThemePreviewMessage => MessageWriter |text| {
        ThemePreviewMessage::Star => text.literal("Estrela"),
        ThemePreviewMessage::Clone => text.literal("Clonar"),
        ThemePreviewMessage::Code => text.literal("Código"),
        ThemePreviewMessage::Issues => text.literal("Issues"),
        ThemePreviewMessage::PullRequests => text.literal("Pull requests"),
        ThemePreviewMessage::Releases => text.literal("Releases"),
        ThemePreviewMessage::Lines { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " linha",
                _ => " linhas",
            });
        }
        ThemePreviewMessage::ChecksPassed => text.literal("Verificações aprovadas"),
        ThemePreviewMessage::ReviewPending => text.literal("Revisão pendente"),
        ThemePreviewMessage::Failing { count } => {
            text.number(count);
            text.literal(" com falha");
        }
        ThemePreviewMessage::Tagline => text.literal("Git auto-hospedado, rápido e silencioso."),
        ThemePreviewMessage::Link => text.literal("Saiba mais"),
        ThemePreviewMessage::Updated => text.literal("Atualizado há 2 horas"),
    }
}
