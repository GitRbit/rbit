//! German (`de`) entries for `catalog/collab/checks.rs`.

use crate::catalog::collab::checks::ChecksMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { De:
    ChecksMessage => MessageWriter |text| {
        ChecksMessage::Summary { successful, failing, pending, expected } => {
            summary(text, successful, failing, pending, expected);
        }
        ChecksMessage::Success => text.literal("Erfolgreich"),
        ChecksMessage::Failure => text.literal("Fehlgeschlagen"),
        ChecksMessage::Error => text.literal("Fehler"),
        ChecksMessage::Pending => text.literal("Ausstehend"),
        ChecksMessage::Expected => text.literal("Erwartet"),
        ChecksMessage::ExpectedDescription => text.literal("Wartet darauf, dass dieser Status gemeldet wird"),
        ChecksMessage::Required => text.literal("Erforderlich"),
        ChecksMessage::Details => text.literal("Details"),
    }
}

/// „2 erfolgreiche und 1 fehlgeschlagene Prüfung, 1 erwartet“.
fn summary(text: &mut MessageWriter, successful: u64, failing: u64, pending: u64, expected: u64) {
    let parts: Vec<(u64, &str)> = [(successful, " erfolgreiche"), (failing, " fehlgeschlagene"), (pending, " ausstehende")]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .collect();
    if parts.is_empty() {
        text.literal("Keine Prüfungen");
    }
    for (index, (count, word)) in parts.iter().enumerate() {
        if index > 0 {
            text.literal(if index + 1 == parts.len() { " und " } else { ", " });
        }
        text.number(*count);
        text.literal(word);
    }
    if !parts.is_empty() {
        text.literal(match text.plural(successful + failing + pending) {
            Plural::One => " Prüfung",
            _ => " Prüfungen",
        });
    }
    if expected > 0 {
        text.literal(", ");
        text.number(expected);
        text.literal(" erwartet");
    }
}
