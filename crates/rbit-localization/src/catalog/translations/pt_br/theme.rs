//! Brazilian Portuguese (`pt-BR`) entries for `catalog/theme.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme::ThemeMessage;
use crate::catalog::theme::ThemeSentence;
use crate::plural::Plural;

entries! { PtBr:
    ThemeMessage => MessageWriter |text| {
        ThemeMessage::Choose => text.literal("Escolher um tema"),
        ThemeMessage::Search => text.literal("Pesquisar temas"),
        ThemeMessage::Close => text.literal("Fechar"),
        ThemeMessage::Show => text.literal("Mostrar"),
        ThemeMessage::All => text.literal("Todos"),
        ThemeMessage::Dark => text.literal("Escuro"),
        ThemeMessage::Light => text.literal("Claro"),
        ThemeMessage::Colorblind => text.literal("Daltonismo"),
        ThemeMessage::Families => text.literal("Famílias de temas"),
        ThemeMessage::Recent => text.literal("Recentes"),
        ThemeMessage::Themes => text.literal("Temas"),
        ThemeMessage::Empty => text.literal("Nenhum tema corresponde à sua pesquisa."),
        ThemeMessage::Preview => text.literal("Prévia"),
        ThemeMessage::System => text.literal("Sistema"),
        ThemeMessage::BuiltIn => text.literal("Integrados"),
        ThemeMessage::More => text.literal("Mais temas"),
        ThemeMessage::Auto => text.literal("Automático"),
        ThemeMessage::Apply => text.literal("Aplicar"),
        ThemeMessage::Applied => text.literal("Aplicado"),
        ThemeMessage::By => text.literal("Por"),
        ThemeMessage::Credit => text.literal("Integrado ao rbit"),
        ThemeMessage::ThemeSingular => text.literal("tema"),
        ThemeMessage::ThemePlural => text.literal("temas"),
        ThemeMessage::OrApply => text.literal("ou clique em Aplicar"),
    }
    ThemeSentence => RichWriter |text| {
        ThemeSentence::Count { count } => {
            text.slot(0);
            text.literal(match text.plural(count) {
                Plural::One => " tema",
                _ => " temas",
            });
        }
    }
}
