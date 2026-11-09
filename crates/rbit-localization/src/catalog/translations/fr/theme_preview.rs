//! French (`fr`) entries for `catalog/theme_preview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme_preview::ThemePreviewMessage;
use crate::plural::Plural;

entries! { Fr:
    ThemePreviewMessage => MessageWriter |text| {
        ThemePreviewMessage::Star => text.literal("Étoile"),
        ThemePreviewMessage::Clone => text.literal("Cloner"),
        ThemePreviewMessage::Code => text.literal("Code"),
        ThemePreviewMessage::Issues => text.literal("Tickets"),
        ThemePreviewMessage::PullRequests => text.literal("Pull requests"),
        ThemePreviewMessage::Releases => text.literal("Releases"),
        ThemePreviewMessage::Lines { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " ligne",
                _ => " lignes",
            });
        }
        ThemePreviewMessage::ChecksPassed => text.literal("Vérifications réussies"),
        ThemePreviewMessage::ReviewPending => text.literal("Revue en attente"),
        ThemePreviewMessage::Failing { count } => {
            text.number(count);
            text.literal(" en échec");
        }
        ThemePreviewMessage::Tagline => text.literal("Git auto-hébergé, rapide et discret."),
        ThemePreviewMessage::Link => text.literal("En savoir plus"),
        ThemePreviewMessage::Updated => text.literal("Mis à jour il y a 2\u{a0}heures"),
    }
}
