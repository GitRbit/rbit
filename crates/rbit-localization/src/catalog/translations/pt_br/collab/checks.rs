//! Brazilian Portuguese (`pt-BR`) entries for `catalog/collab/checks.rs`.

use crate::catalog::collab::checks::ChecksMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { PtBr:
    ChecksMessage => MessageWriter |text| {
        ChecksMessage::Summary { successful, failing, pending, expected } => {
            summary(text, [successful, failing, pending], expected);
        }
        ChecksMessage::Success => text.literal("Sucesso"),
        ChecksMessage::Failure => text.literal("Falha"),
        ChecksMessage::Error => text.literal("Erro"),
        ChecksMessage::Pending => text.literal("Pendente"),
        ChecksMessage::Expected => text.literal("Esperada"),
        ChecksMessage::ExpectedDescription => text.literal("Aguardando o envio deste status"),
        ChecksMessage::Required => text.literal("Obrigatória"),
        ChecksMessage::Details => text.literal("Detalhes"),
    }
}

/// “Verificações: 2 bem-sucedidas, 1 com falha e 1 pendente, 1 esperada”.
fn summary(text: &mut MessageWriter, [successful, failing, pending]: [u64; 3], expected: u64) {
    let words: [(u64, &str, &str); 3] =
        [(successful, " bem-sucedida", " bem-sucedidas"), (failing, " com falha", " com falha"), (pending, " pendente", " pendentes")];
    let parts: Vec<(u64, &str, &str)> = words.into_iter().filter(|(count, ..)| *count > 0).collect();
    if parts.is_empty() {
        text.literal("Nenhuma verificação");
    } else {
        let total = successful + failing + pending;
        text.literal(match text.plural(total) {
            Plural::One => "Verificação: ",
            _ => "Verificações: ",
        });
    }
    for (index, &(count, one, other)) in parts.iter().enumerate() {
        if index > 0 {
            text.literal(if index + 1 == parts.len() { " e " } else { ", " });
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
            Plural::One => " esperada",
            _ => " esperadas",
        });
    }
}
