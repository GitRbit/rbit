//! Lazy compiled labels for the shared picker. Templates show each through `ui::localized`,
//! which marks an untranslated label `lang="en"`.
use rbit_localization::{Catalog, Locale, Localized, ThemeMessage as Message, ThemePreviewMessage as Preview};

#[derive(Debug, Clone, Copy)]
pub struct PickerLabels(Catalog);
impl PickerLabels {
    pub const fn new(locale: Locale) -> Self {
        Self(Catalog::new(locale))
    }
    pub fn choose(self) -> Localized {
        self.0.localized(Message::Choose)
    }
    pub fn search(self) -> Localized {
        self.0.localized(Message::Search)
    }
    pub fn close(self) -> Localized {
        self.0.localized(Message::Close)
    }
    pub fn show(self) -> Localized {
        self.0.localized(Message::Show)
    }
    pub fn all(self) -> Localized {
        self.0.localized(Message::All)
    }
    pub fn dark(self) -> Localized {
        self.0.localized(Message::Dark)
    }
    pub fn light(self) -> Localized {
        self.0.localized(Message::Light)
    }
    pub fn colorblind(self) -> Localized {
        self.0.localized(Message::Colorblind)
    }
    pub fn families(self) -> Localized {
        self.0.localized(Message::Families)
    }
    pub fn recent(self) -> Localized {
        self.0.localized(Message::Recent)
    }
    pub fn themes(self) -> Localized {
        self.0.localized(Message::Themes)
    }
    pub fn empty(self) -> Localized {
        self.0.localized(Message::Empty)
    }
    pub fn preview(self) -> Localized {
        self.0.localized(Message::Preview)
    }
    pub fn system(self) -> Localized {
        self.0.localized(Message::System)
    }
    pub fn built_in(self) -> Localized {
        self.0.localized(Message::BuiltIn)
    }
    pub fn more(self) -> Localized {
        self.0.localized(Message::More)
    }
    pub fn auto(self) -> Localized {
        self.0.localized(Message::Auto)
    }
    pub fn apply(self) -> Localized {
        self.0.localized(Message::Apply)
    }
    pub fn applied(self) -> Localized {
        self.0.localized(Message::Applied)
    }
    pub fn by(self) -> Localized {
        self.0.localized(Message::By)
    }
    pub fn credit(self) -> Localized {
        self.0.localized(Message::Credit)
    }
    pub fn theme_singular(self) -> Localized {
        self.0.localized(Message::ThemeSingular)
    }
    pub fn theme_plural(self) -> Localized {
        self.0.localized(Message::ThemePlural)
    }
    pub fn search_prefix(self) -> Localized {
        self.0.localized(Message::SearchPrefix)
    }
    pub fn or_apply(self) -> Localized {
        self.0.localized(Message::OrApply)
    }
    pub fn variant(self, value: &str) -> Localized {
        self.0.localized(match value {
            "dark" => Message::Dark,
            "light" => Message::Light,
            _ => Message::Auto,
        })
    }

    /// A label of the miniature page, or its English fallback.
    pub fn preview_text(self, message: Preview) -> Localized {
        self.0.localized(message)
    }

    /// The miniature file's line count.
    pub fn preview_lines(self, count: u64) -> Localized {
        self.0.localized(Preview::Lines { count })
    }

    /// The miniature page's failing-check count.
    pub fn preview_failing(self, count: u64) -> Localized {
        self.0.localized(Preview::Failing { count })
    }
}
