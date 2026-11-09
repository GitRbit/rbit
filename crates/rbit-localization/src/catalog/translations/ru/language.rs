//! Russian (`ru`) entries for `catalog/language.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::language::LanguageMessage;

entries! { Ru:
    LanguageMessage => MessageWriter |text| {
        LanguageMessage::Title => text.literal("Язык интерфейса"),
        LanguageMessage::Label => text.literal("Предпочитаемый язык"),
        LanguageMessage::BrowserDefault => text.literal("Браузер / по умолчанию"),
        LanguageMessage::BrowserHelp => text.literal("Использовать поддерживаемый язык браузера, а если он недоступен — английский."),
        LanguageMessage::Coverage => text.literal("Текст, который ещё не переведён на ваш язык, показывается на английском."),
        LanguageMessage::English => text.literal("English"),
        LanguageMessage::Spanish => text.literal("Español"),
        LanguageMessage::Save => text.literal("Сохранить язык"),
        LanguageMessage::Saved => text.literal("Предпочитаемый язык сохранён."),
        LanguageMessage::Invalid => text.literal("Выберите один из перечисленных языков или язык браузера."),
    }
}
