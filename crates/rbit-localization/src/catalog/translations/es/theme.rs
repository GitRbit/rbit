//! Spanish (`es`) entries for `catalog/theme.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme::ThemeMessage;

entries! { Es:
    ThemeMessage => MessageWriter |text| {
        ThemeMessage::Choose => text.literal("Elegir un tema"),
        ThemeMessage::Search => text.literal("Buscar temas"),
        ThemeMessage::Close => text.literal("Cerrar"),
        ThemeMessage::Show => text.literal("Mostrar"),
        ThemeMessage::All => text.literal("Todos"),
        ThemeMessage::Dark => text.literal("Oscuro"),
        ThemeMessage::Light => text.literal("Claro"),
        ThemeMessage::Colorblind => text.literal("Daltonismo"),
        ThemeMessage::Families => text.literal("Familias de temas"),
        ThemeMessage::Recent => text.literal("Recientes"),
        ThemeMessage::Themes => text.literal("Temas"),
        ThemeMessage::Empty => text.literal("Ningún tema coincide con tu búsqueda."),
        ThemeMessage::Preview => text.literal("Vista previa"),
        ThemeMessage::System => text.literal("Sistema"),
        ThemeMessage::BuiltIn => text.literal("Integrados"),
        ThemeMessage::More => text.literal("Más temas"),
        ThemeMessage::Auto => text.literal("Automático"),
        ThemeMessage::Apply => text.literal("Aplicar"),
        ThemeMessage::Applied => text.literal("Aplicado"),
        ThemeMessage::By => text.literal("Por"),
        ThemeMessage::Credit => text.literal("Integrado en rbit"),
        ThemeMessage::ThemeSingular => text.literal("tema"),
        ThemeMessage::ThemePlural => text.literal("temas"),
        ThemeMessage::SearchPrefix => text.literal("Buscar"),
        ThemeMessage::OrApply => text.literal("o pulsa Aplicar"),
    }
}
