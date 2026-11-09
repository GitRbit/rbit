//! Compiled theme picker labels; theme identifiers and user content stay outside the catalog.
use super::fallback::{MessageFamily, MessageWriter};
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
    OrApply,
}
impl ThemeMessage {
    /// Catalog identifier, independent of the text.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Choose => "themes.picker.choose",
            Self::Search => "themes.picker.search",
            Self::Close => "themes.picker.close",
            Self::Show => "themes.picker.show",
            Self::All => "themes.picker.all",
            Self::Dark => "themes.picker.dark",
            Self::Light => "themes.picker.light",
            Self::Colorblind => "themes.picker.colorblind",
            Self::Families => "themes.picker.families",
            Self::Recent => "themes.picker.recent",
            Self::Themes => "themes.picker.themes",
            Self::Empty => "themes.picker.empty",
            Self::Preview => "themes.picker.preview",
            Self::System => "themes.picker.system",
            Self::BuiltIn => "themes.picker.built-in",
            Self::More => "themes.picker.more",
            Self::Auto => "themes.picker.auto",
            Self::Apply => "themes.picker.apply",
            Self::Applied => "themes.picker.applied",
            Self::By => "themes.picker.by",
            Self::Credit => "themes.picker.credit",
            Self::ThemeSingular => "themes.picker.theme-singular",
            Self::ThemePlural => "themes.picker.theme-plural",
            Self::OrApply => "themes.picker.or-apply",
        }
    }
}

impl ThemeMessage {
    /// The English text.
    const fn english_label(self) -> &'static str {
        match self {
            Self::Choose => "Choose a theme",
            Self::Search => "Search themes",
            Self::Close => "Close",
            Self::Show => "Show",
            Self::All => "All",
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Colorblind => "Colorblind",
            Self::Families => "Theme families",
            Self::Recent => "Recent",
            Self::Themes => "Themes",
            Self::Empty => "No themes match your search.",
            Self::Preview => "Preview",
            Self::System => "System",
            Self::BuiltIn => "Built in",
            Self::More => "More themes",
            Self::Auto => "Auto",
            Self::Apply => "Apply",
            Self::Applied => "Applied",
            Self::By => "By",
            Self::Credit => "Built into rbit",
            Self::ThemeSingular => "theme",
            Self::ThemePlural => "themes",
            Self::OrApply => "or click Apply",
        }
    }
}

impl MessageFamily for ThemeMessage {
    fn id(self) -> &'static str {
        Self::id(self)
    }

    fn english(self, text: &mut MessageWriter) {
        text.literal(self.english_label());
    }

    fn english_literal(self) -> Option<&'static str> {
        Some(self.english_label())
    }

    fn translate(self, locale: Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

/// Theme picker sentences that embed markup; the template renders each slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeSentence {
    /// How many themes are shown. Slot 0: the count, as digits.
    Count { count: u64 },
}

impl ThemeSentence {
    /// This sentence in `locale`, or its English text marked as a fallback.
    pub fn rich(self, locale: impl std::borrow::Borrow<Locale>) -> crate::catalog::collab::rich::Rich {
        crate::catalog::collab::rich::render(*locale.borrow(), self)
    }
}

impl crate::catalog::collab::rich::RichFamily for ThemeSentence {
    fn id(self) -> &'static str {
        match self {
            Self::Count { .. } => "themes.picker.count",
        }
    }

    fn english(self, text: &mut crate::catalog::collab::rich::RichWriter) {
        match self {
            Self::Count { count } => {
                text.slot(0);
                text.literal(if count == 1 { " theme" } else { " themes" });
            }
        }
    }

    fn translate(self, locale: Locale, text: &mut crate::catalog::collab::rich::RichWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
