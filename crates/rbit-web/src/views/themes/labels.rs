//! Lazy compiled labels for the shared picker, with no per-row formatted strings.
use rbit_localization::{Catalog, Locale, Localized, LocalizedText, ThemeMessage as Message, ThemePreviewMessage as Preview};

#[derive(Debug, Clone, Copy)]
pub struct PickerLabels(Catalog);
impl PickerLabels {
    pub const fn new(locale: Locale) -> Self {
        Self(Catalog::new(locale))
    }
    pub fn choose(self) -> LocalizedText {
        self.0.theme(Message::Choose)
    }
    pub fn search(self) -> LocalizedText {
        self.0.theme(Message::Search)
    }
    pub fn close(self) -> LocalizedText {
        self.0.theme(Message::Close)
    }
    pub fn show(self) -> LocalizedText {
        self.0.theme(Message::Show)
    }
    pub fn all(self) -> LocalizedText {
        self.0.theme(Message::All)
    }
    pub fn dark(self) -> LocalizedText {
        self.0.theme(Message::Dark)
    }
    pub fn light(self) -> LocalizedText {
        self.0.theme(Message::Light)
    }
    pub fn colorblind(self) -> LocalizedText {
        self.0.theme(Message::Colorblind)
    }
    pub fn families(self) -> LocalizedText {
        self.0.theme(Message::Families)
    }
    pub fn recent(self) -> LocalizedText {
        self.0.theme(Message::Recent)
    }
    pub fn themes(self) -> LocalizedText {
        self.0.theme(Message::Themes)
    }
    pub fn empty(self) -> LocalizedText {
        self.0.theme(Message::Empty)
    }
    pub fn preview(self) -> LocalizedText {
        self.0.theme(Message::Preview)
    }
    pub fn system(self) -> LocalizedText {
        self.0.theme(Message::System)
    }
    pub fn built_in(self) -> LocalizedText {
        self.0.theme(Message::BuiltIn)
    }
    pub fn more(self) -> LocalizedText {
        self.0.theme(Message::More)
    }
    pub fn auto(self) -> LocalizedText {
        self.0.theme(Message::Auto)
    }
    pub fn apply(self) -> LocalizedText {
        self.0.theme(Message::Apply)
    }
    pub fn applied(self) -> LocalizedText {
        self.0.theme(Message::Applied)
    }
    pub fn by(self) -> LocalizedText {
        self.0.theme(Message::By)
    }
    pub fn credit(self) -> LocalizedText {
        self.0.theme(Message::Credit)
    }
    pub fn theme_singular(self) -> LocalizedText {
        self.0.theme(Message::ThemeSingular)
    }
    pub fn theme_plural(self) -> LocalizedText {
        self.0.theme(Message::ThemePlural)
    }
    pub fn search_prefix(self) -> LocalizedText {
        self.0.theme(Message::SearchPrefix)
    }
    pub fn or_apply(self) -> LocalizedText {
        self.0.theme(Message::OrApply)
    }
    pub fn variant(self, value: &str) -> LocalizedText {
        self.0.theme(match value {
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
