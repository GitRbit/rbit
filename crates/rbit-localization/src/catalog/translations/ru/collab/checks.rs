//! Russian (`ru`) entries for `catalog/collab/checks.rs`.

use crate::catalog::collab::checks::ChecksMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ru:
    ChecksMessage => MessageWriter |text| {
        ChecksMessage::Summary { successful, failing, pending, expected } => {
            let parts: Vec<(u64, &str)> = [(successful, " успешно"), (failing, " с ошибкой"), (pending, " в ожидании")]
                .into_iter()
                .filter(|(count, _)| *count > 0)
                .collect();
            text.literal(if parts.is_empty() { "Нет проверок" } else { "Проверки: " });
            for (index, (count, word)) in parts.iter().enumerate() {
                if index > 0 {
                    text.literal(if index + 1 == parts.len() { " и " } else { ", " });
                }
                text.number(*count);
                text.literal(word);
            }
            if expected > 0 {
                text.literal(", ожидается: ");
                text.number(expected);
            }
        }
        ChecksMessage::Success => text.literal("Успешно"),
        ChecksMessage::Failure => text.literal("Ошибка"),
        ChecksMessage::Error => text.literal("Сбой"),
        ChecksMessage::Pending => text.literal("В ожидании"),
        ChecksMessage::Expected => text.literal("Ожидается"),
        ChecksMessage::ExpectedDescription => text.literal("Ожидание отчёта об этом статусе"),
        ChecksMessage::Required => text.literal("Обязательная"),
        ChecksMessage::Details => text.literal("Подробности"),
    }
}
