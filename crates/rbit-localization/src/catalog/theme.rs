//! Compiled theme picker labels; theme identifiers and user content stay outside the catalog.
use super::{Catalog, LocalizedText};
use crate::Locale;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeMessage {
    Choose,
    Search,
    Close,
    Show,
    All,
    Dark,
    Light,
    Colorblind,
    Families,
    Recent,
    Themes,
    Empty,
    Preview,
    System,
    BuiltIn,
    More,
    Auto,
    Apply,
    Applied,
    By,
    Credit,
    ThemeSingular,
    ThemePlural,
    SearchPrefix,
    OrApply,
}
impl Catalog {
    pub fn theme(self, message: ThemeMessage) -> LocalizedText {
        let (english, spanish) = match message {
            ThemeMessage::Choose => ("Choose a theme", "Elegir un tema"),
            ThemeMessage::Search => ("Search themes", "Buscar temas"),
            ThemeMessage::Close => ("Close", "Cerrar"),
            ThemeMessage::Show => ("Show", "Mostrar"),
            ThemeMessage::All => ("All", "Todos"),
            ThemeMessage::Dark => ("Dark", "Oscuro"),
            ThemeMessage::Light => ("Light", "Claro"),
            ThemeMessage::Colorblind => ("Colorblind", "Daltonismo"),
            ThemeMessage::Families => ("Theme families", "Familias de temas"),
            ThemeMessage::Recent => ("Recent", "Recientes"),
            ThemeMessage::Themes => ("Themes", "Temas"),
            ThemeMessage::Empty => ("No themes match your search.", "Ningún tema coincide con tu búsqueda."),
            ThemeMessage::Preview => ("Preview", "Vista previa"),
            ThemeMessage::System => ("System", "Sistema"),
            ThemeMessage::BuiltIn => ("Built in", "Integrados"),
            ThemeMessage::More => ("More themes", "Más temas"),
            ThemeMessage::Auto => ("Auto", "Automático"),
            ThemeMessage::Apply => ("Apply", "Aplicar"),
            ThemeMessage::Applied => ("Applied", "Aplicado"),
            ThemeMessage::By => ("By", "Por"),
            ThemeMessage::Credit => ("Built into rbit", "Integrado en rbit"),
            ThemeMessage::ThemeSingular => ("theme", "tema"),
            ThemeMessage::ThemePlural => ("themes", "temas"),
            ThemeMessage::SearchPrefix => ("Search", "Buscar"),
            ThemeMessage::OrApply => ("or click Apply", "o pulsa Aplicar"),
        };
        self.literal_text(if self.locale == Locale::Spanish { spanish } else { english })
    }
}
