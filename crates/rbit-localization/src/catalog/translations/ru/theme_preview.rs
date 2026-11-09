//! Russian (`ru`) entries for `catalog/theme_preview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme_preview::ThemePreviewMessage;
use crate::plural::Plural;

entries! { Ru:
    ThemePreviewMessage => MessageWriter |text| {
        ThemePreviewMessage::Star => text.literal("Звезда"),
        ThemePreviewMessage::Clone => text.literal("Клонировать"),
        ThemePreviewMessage::Code => text.literal("Код"),
        ThemePreviewMessage::Issues => text.literal("Задачи"),
        ThemePreviewMessage::PullRequests => text.literal("Запросы на слияние"),
        ThemePreviewMessage::Releases => text.literal("Релизы"),
        ThemePreviewMessage::Lines { count } => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "строка",
                Plural::Few => "строки",
                _ => "строк",
            });
        }
        ThemePreviewMessage::ChecksPassed => text.literal("Проверки пройдены"),
        ThemePreviewMessage::ReviewPending => text.literal("Ожидает ревью"),
        ThemePreviewMessage::Failing { count } => {
            text.literal("Не пройдено: ");
            text.number(count);
        }
        ThemePreviewMessage::Tagline => text.literal("Собственный Git-сервер — быстрый и тихий."),
        ThemePreviewMessage::Link => text.literal("Подробнее"),
        ThemePreviewMessage::Updated => text.literal("Обновлено 2 часа назад"),
    }
}
