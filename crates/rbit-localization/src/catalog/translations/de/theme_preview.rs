//! German (`de`) entries for `catalog/theme_preview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme_preview::ThemePreviewMessage;
use crate::plural::Plural;

entries! { De:
    ThemePreviewMessage => MessageWriter |text| {
        ThemePreviewMessage::Star => text.literal("Stern"),
        ThemePreviewMessage::Clone => text.literal("Klonen"),
        ThemePreviewMessage::Code => text.literal("Code"),
        ThemePreviewMessage::Issues => text.literal("Issues"),
        ThemePreviewMessage::PullRequests => text.literal("Pull-Requests"),
        ThemePreviewMessage::Releases => text.literal("Releases"),
        ThemePreviewMessage::Lines { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Zeile",
                _ => " Zeilen",
            });
        }
        ThemePreviewMessage::ChecksPassed => text.literal("Prüfungen bestanden"),
        ThemePreviewMessage::ReviewPending => text.literal("Review ausstehend"),
        ThemePreviewMessage::Failing { count } => {
            text.number(count);
            text.literal(" fehlgeschlagen");
        }
        ThemePreviewMessage::Tagline => text.literal("Selbst gehostetes Git, schnell und leise."),
        ThemePreviewMessage::Link => text.literal("Weiterlesen"),
        ThemePreviewMessage::Updated => text.literal("Vor 2 Stunden aktualisiert"),
    }
}
