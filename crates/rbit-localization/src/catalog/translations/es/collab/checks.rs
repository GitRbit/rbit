//! Spanish (`es`) entries for `catalog/collab/checks.rs`.

use crate::catalog::collab::checks::ChecksMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Es:
    ChecksMessage => MessageWriter |text| {
        ChecksMessage::Summary { successful, failing, pending, expected } => {
            summary(text, [successful, failing, pending], expected);
        }
        ChecksMessage::Success => text.literal("Correcta"),
        ChecksMessage::Failure => text.literal("Fallida"),
        ChecksMessage::Error => text.literal("Error"),
        ChecksMessage::Pending => text.literal("Pendiente"),
        ChecksMessage::Expected => text.literal("Esperada"),
        ChecksMessage::ExpectedDescription => text.literal("Esperando a que se informe este estado"),
        ChecksMessage::Required => text.literal("Obligatoria"),
        ChecksMessage::Details => text.literal("Detalles"),
    }
}

/// "Comprobaciones: 2 correctas, 1 fallida y 3 pendientes; 1 esperada".
fn summary(text: &mut MessageWriter, counts: [u64; 3], expected: u64) {
    const WORDS: [(&str, &str); 3] = [(" correcta", " correctas"), (" fallida", " fallidas"), (" pendiente", " pendientes")];
    let parts: Vec<(u64, (&str, &str))> = counts.into_iter().zip(WORDS).filter(|(count, _)| *count > 0).collect();
    if parts.is_empty() {
        text.literal("Sin comprobaciones");
    } else {
        text.literal("Comprobaciones: ");
    }
    for (index, (count, (one, other))) in parts.iter().enumerate() {
        if index > 0 {
            text.literal(if index + 1 == parts.len() { " y " } else { ", " });
        }
        text.number(*count);
        text.literal(if text.plural(*count) == Plural::One { one } else { other });
    }
    if expected > 0 {
        text.literal("; ");
        text.number(expected);
        text.literal(if text.plural(expected) == Plural::One { " esperada" } else { " esperadas" });
    }
}
