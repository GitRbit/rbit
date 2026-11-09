//! French (`fr`) entries for `catalog/collab/checks.rs`.

use crate::catalog::collab::checks::ChecksMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Fr:
    ChecksMessage => MessageWriter |text| {
        ChecksMessage::Summary { successful, failing, pending, expected } => {
            summary(text, [successful, failing, pending], expected);
        }
        ChecksMessage::Success => text.literal("Réussie"),
        ChecksMessage::Failure => text.literal("Échec"),
        ChecksMessage::Error => text.literal("Erreur"),
        ChecksMessage::Pending => text.literal("En attente"),
        ChecksMessage::Expected => text.literal("Attendue"),
        ChecksMessage::ExpectedDescription => text.literal("En attente du signalement de ce statut"),
        ChecksMessage::Required => text.literal("Requise"),
        ChecksMessage::Details => text.literal("Détails"),
    }
}

/// « Vérifications : 2 réussies, 1 en échec et 3 en attente, 1 attendue ».
fn summary(text: &mut MessageWriter, [successful, failing, pending]: [u64; 3], expected: u64) {
    let parts: Vec<(u64, &str, &str)> =
        [(successful, " réussie", " réussies"), (failing, " en échec", " en échec"), (pending, " en attente", " en attente")]
            .into_iter()
            .filter(|(count, ..)| *count > 0)
            .collect();
    if parts.is_empty() {
        text.literal("Aucune vérification");
    } else {
        text.literal(match text.plural(successful + failing + pending) {
            Plural::One => "Vérification\u{a0}: ",
            _ => "Vérifications\u{a0}: ",
        });
    }
    for (index, &(count, one, other)) in parts.iter().enumerate() {
        if index > 0 {
            text.literal(if index + 1 == parts.len() { " et " } else { ", " });
        }
        text.number(count);
        text.literal(match text.plural(count) {
            Plural::One => one,
            _ => other,
        });
    }
    if expected > 0 {
        text.literal(", ");
        text.number(expected);
        text.literal(match text.plural(expected) {
            Plural::One => " attendue",
            _ => " attendues",
        });
    }
}
