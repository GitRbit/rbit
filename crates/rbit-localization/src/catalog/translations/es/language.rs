//! Spanish (`es`) entries for `catalog/language.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::language::LanguageMessage;

entries! { Es:
    LanguageMessage => MessageWriter |text| {
        LanguageMessage::Title => text.literal("Idioma de la interfaz"),
        LanguageMessage::Label => text.literal("Idioma preferido"),
        LanguageMessage::BrowserDefault => text.literal("Navegador / predeterminado"),
        LanguageMessage::BrowserHelp => text.literal("Usa un idioma compatible de tu navegador, o inglés si no está disponible."),
        LanguageMessage::Coverage => {
            text.literal("El texto que aún no está traducido a tu idioma se muestra en inglés.");
        }
        LanguageMessage::English => text.literal("English"),
        LanguageMessage::Spanish => text.literal("Español"),
        LanguageMessage::Save => text.literal("Guardar idioma"),
        LanguageMessage::Saved => text.literal("Se guardó tu idioma preferido."),
        LanguageMessage::Invalid => text.literal("Elige uno de los idiomas de la lista o la opción del navegador."),
    }
}
