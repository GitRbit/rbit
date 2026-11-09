//! Brazilian Portuguese (`pt-BR`) entries for `catalog/language.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::language::LanguageMessage;

entries! { PtBr:
    LanguageMessage => MessageWriter |text| {
        LanguageMessage::Title => text.literal("Idioma da interface"),
        LanguageMessage::Label => text.literal("Idioma preferido"),
        LanguageMessage::BrowserDefault => text.literal("Navegador / padrão"),
        LanguageMessage::BrowserHelp => text.literal("Segue o idioma compatível do seu navegador ou usa inglês quando ele não está disponível."),
        LanguageMessage::Coverage => text.literal("O texto que ainda não foi traduzido para o seu idioma aparece em inglês."),
        LanguageMessage::English => text.literal("English"),
        LanguageMessage::Spanish => text.literal("Español"),
        LanguageMessage::Save => text.literal("Salvar idioma"),
        LanguageMessage::Saved => text.literal("Sua preferência de idioma foi salva."),
        LanguageMessage::Invalid => text.literal("Escolha um dos idiomas listados ou o padrão do navegador."),
    }
}
