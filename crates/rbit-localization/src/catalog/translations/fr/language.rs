//! French (`fr`) entries for `catalog/language.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::language::LanguageMessage;

entries! { Fr:
    LanguageMessage => MessageWriter |text| {
        LanguageMessage::Title => text.literal("Langue d’affichage"),
        LanguageMessage::Label => text.literal("Langue préférée"),
        LanguageMessage::BrowserDefault => text.literal("Navigateur / par défaut"),
        LanguageMessage::BrowserHelp => text.literal(
            "Utilise une langue prise en charge de votre navigateur, ou l’anglais si elle n’est pas disponible.",
        ),
        LanguageMessage::Coverage => {
            text.literal("Le texte qui n’est pas encore traduit dans votre langue s’affiche en anglais.");
        }
        LanguageMessage::English => text.literal("English"),
        LanguageMessage::Spanish => text.literal("Español"),
        LanguageMessage::Save => text.literal("Enregistrer la langue"),
        LanguageMessage::Saved => text.literal("Votre langue préférée a été enregistrée."),
        LanguageMessage::Invalid => {
            text.literal("Choisissez l’une des langues proposées ou l’option du navigateur.");
        }
        LanguageMessage::Close => text.literal("Fermer"),
        LanguageMessage::DeviceOnly => text.literal("Ce navigateur mémorise votre choix dans un cookie qui ne contient que la langue."),
    }
}
