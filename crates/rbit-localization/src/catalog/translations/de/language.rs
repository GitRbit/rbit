//! German (`de`) entries for `catalog/language.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::language::LanguageMessage;

entries! { De:
    LanguageMessage => MessageWriter |text| {
        LanguageMessage::Title => text.literal("Anzeigesprache"),
        LanguageMessage::Label => text.literal("Bevorzugte Sprache"),
        LanguageMessage::BrowserDefault => text.literal("Browser / Standard"),
        LanguageMessage::BrowserHelp => text.literal(
            "Verwendet eine unterstützte Sprache Ihres Browsers oder Englisch, wenn keine verfügbar ist.",
        ),
        LanguageMessage::Coverage => {
            text.literal("Text, der noch nicht in Ihre Sprache übersetzt ist, wird auf Englisch angezeigt.");
        }
        LanguageMessage::English => text.literal("English"),
        LanguageMessage::Spanish => text.literal("Español"),
        LanguageMessage::Save => text.literal("Sprache speichern"),
        LanguageMessage::Saved => text.literal("Ihre bevorzugte Sprache wurde gespeichert."),
        LanguageMessage::Invalid => {
            text.literal("Wählen Sie eine der aufgeführten Sprachen oder die Browser-Einstellung.");
        }
        LanguageMessage::Close => text.literal("Schließen"),
        LanguageMessage::DeviceOnly => {
            text.literal("Dieser Browser merkt sich Ihre Auswahl in einem Cookie, das nur die Sprache enthält.");
        }
    }
}
