//! Spanish (`es`) entries for `catalog/theme_preview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme_preview::ThemePreviewMessage;
use crate::plural::Plural;

entries! { Es:
    ThemePreviewMessage => MessageWriter |text| {
        ThemePreviewMessage::Star => text.literal("Estrella"),
        ThemePreviewMessage::Clone => text.literal("Clonar"),
        ThemePreviewMessage::Code => text.literal("Código"),
        ThemePreviewMessage::Issues => text.literal("Incidencias"),
        ThemePreviewMessage::PullRequests => text.literal("Solicitudes de cambios"),
        ThemePreviewMessage::Releases => text.literal("Versiones"),
        ThemePreviewMessage::Lines { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " línea",
                _ => " líneas",
            });
        }
        ThemePreviewMessage::ChecksPassed => text.literal("Comprobaciones superadas"),
        ThemePreviewMessage::ReviewPending => text.literal("Revisión pendiente"),
        ThemePreviewMessage::Failing { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " fallida",
                _ => " fallidas",
            });
        }
        ThemePreviewMessage::Tagline => text.literal("Git autoalojado, rápido y silencioso."),
        ThemePreviewMessage::Link => text.literal("Más información"),
        ThemePreviewMessage::Updated => text.literal("Actualizado hace 2 horas"),
    }
}
