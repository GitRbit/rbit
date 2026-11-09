//! Lazy compiled labels for the shared picker. Templates show each through `ui::localized`,
//! which marks an untranslated label `lang="en"`.
use std::borrow::Borrow;

use rbit_localization::settings::profile::GalleryText;
use rbit_localization::{Catalog, Locale, Localized, Plural, Rich, ThemeMessage as Message, ThemePreviewMessage as Preview, ThemeSentence};

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
    /// The search field's placeholder: the whole sentence, with the count in place.
    pub fn search_count(self, total: impl Borrow<usize>) -> Localized {
        GalleryText::SearchCount(u64::try_from(*total.borrow()).unwrap_or(u64::MAX)).text(&self.0.locale())
    }
    /// How many themes are shown; slot 0 is the count.
    pub fn count(self, shown: impl Borrow<usize>) -> Rich {
        ThemeSentence::Count { count: u64::try_from(*shown.borrow()).unwrap_or(u64::MAX) }.rich(self.0.locale())
    }
    /// The count sentence for each CLDR plural category of the locale, with `{n}` where the
    /// number goes: `theme-picker-labels.js` fills in live counts with the category's form.
    pub fn count_forms(self) -> Vec<(&'static str, String)> {
        let locale = self.0.locale();
        Plural::categories(locale)
            .iter()
            .filter_map(|&category| {
                let sample = (0..=1_000).chain([1_000_000]).find(|&count| Plural::of(locale, count) == category)?;
                Some((plural_name(category), ThemeSentence::Count { count: sample }.rich(locale).plain(&["{n}"])))
            })
            .collect()
    }
    /// The language whose plural rules choose among [`Self::count_forms`]: English when the
    /// sentence falls back to English.
    pub fn count_plural(self) -> &'static str {
        let locale = self.0.locale();
        let sample = ThemeSentence::Count { count: 2 }.rich(locale);
        if sample.fallback() { Locale::English.tag() } else { locale.tag() }
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

/// The CLDR name of a plural category, as `data-count-*` attributes spell it.
const fn plural_name(category: Plural) -> &'static str {
    match category {
        Plural::Zero => "zero",
        Plural::One => "one",
        Plural::Two => "two",
        Plural::Few => "few",
        Plural::Many => "many",
        Plural::Other => "other",
    }
}
