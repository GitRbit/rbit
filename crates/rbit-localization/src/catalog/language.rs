//! Complete settings messages for the production language preference.
use super::{Catalog, LocalizedText};
use crate::Locale;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LanguageMessage {
    Title,
    Label,
    BrowserDefault,
    BrowserHelp,
    Coverage,
    English,
    Spanish,
    Save,
    Saved,
    Invalid,
}
impl LanguageMessage {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Title => "settings.language.title",
            Self::Label => "settings.language.label",
            Self::BrowserDefault => "settings.language.browser-default",
            Self::BrowserHelp => "settings.language.browser-help",
            Self::Coverage => "settings.language.coverage",
            Self::English => "settings.language.english",
            Self::Spanish => "settings.language.spanish",
            Self::Save => "settings.language.save",
            Self::Saved => "settings.language.saved",
            Self::Invalid => "settings.language.invalid",
        }
    }
}
impl Catalog {
    pub fn language(self, message: LanguageMessage) -> LocalizedText {
        let (english, spanish) = match message {
            LanguageMessage::Title => ("Display language", "Idioma de la interfaz"),
            LanguageMessage::Label => ("Preferred language", "Idioma preferido"),
            LanguageMessage::BrowserDefault => ("Browser / default", "Navegador / predeterminado"),
            LanguageMessage::BrowserHelp => (
                "Follow your browser's supported language, or use English when it is unavailable.",
                "Usa un idioma compatible de tu navegador, o inglés si no está disponible.",
            ),
            LanguageMessage::Coverage => (
                "Spanish covers navigation and the localized Explore and Actions screens. Other content remains in English.",
                "El español cubre la navegación y las pantallas traducidas de Explorar y Acciones. El resto del contenido sigue en inglés.",
            ),
            LanguageMessage::English => ("English", "English"),
            LanguageMessage::Spanish => ("Español", "Español"),
            LanguageMessage::Save => ("Save language", "Guardar idioma"),
            LanguageMessage::Saved => ("Your language preference was saved.", "Se guardó tu idioma preferido."),
            LanguageMessage::Invalid => (
                "Choose one of the listed languages or the browser default.",
                "Elige uno de los idiomas de la lista o la opción del navegador.",
            ),
        };
        self.literal_text(if self.locale == Locale::Spanish { spanish } else { english })
    }
}
