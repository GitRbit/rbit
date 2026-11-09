//! Interface language choice with compiled labels: the account settings page and the same
//! form for visitors who are not signed in (kept in a cookie). An untranslated label is
//! marked `lang="en"` by the template.
use super::{SettingsLink, SettingsSection};
use crate::views::layout::Page;
use askama::Template;
use askama_web::WebTemplate;
use rbit_identity_models::AccountLocale;
use rbit_localization::{Catalog, LanguageMessage as Message, Locale, Localized};

/// `GET /settings/language` for a signed-in user: the saved account choice.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "settings/language.html")]
pub struct LanguagePage {
    pub page: Page,
    pub settings_nav: Vec<SettingsLink>,
    pub selected: &'static str,
    pub saved: bool,
    pub invalid: bool,
}
impl LanguagePage {
    pub fn new(page: Page, selected: Option<AccountLocale>, saved: bool) -> Self {
        let locale = page.locale;
        let title = Catalog::new(locale).localized(Message::Title);
        Self {
            page: page.with_locale(locale).titled(title.as_str()),
            settings_nav: SettingsSection::Language.nav(),
            selected: selected.map_or("", AccountLocale::tag),
            saved,
            invalid: false,
        }
    }

    pub fn labels(&self) -> LanguageText {
        LanguageText::new(self.page.locale)
    }
}

/// `GET /settings/language` for a visitor who is not signed in: the same form without the
/// settings navigation, saved in the `rbit-locale` cookie.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "settings/language_visitor.html")]
pub struct VisitorLanguagePage {
    pub page: Page,
    pub selected: &'static str,
    pub saved: bool,
    pub invalid: bool,
}
impl VisitorLanguagePage {
    /// `selected` is the language the visitor's cookie chose, if any.
    pub fn new(page: Page, selected: Option<Locale>, saved: bool) -> Self {
        let locale = page.locale;
        let title = Catalog::new(locale).localized(Message::Title);
        Self { page: page.with_locale(locale).titled(title.as_str()), selected: selected.map_or("", Locale::tag), saved, invalid: false }
    }

    pub fn labels(&self) -> LanguageText {
        LanguageText::new(self.page.locale)
    }
}

#[derive(Debug)]
pub struct LanguageText {
    pub title: Localized,
    pub label: Localized,
    pub browser: Localized,
    pub help: Localized,
    pub coverage: Localized,
    pub save: Localized,
    pub saved: Localized,
    pub invalid: Localized,
    pub device_only: Localized,
}
impl LanguageText {
    fn new(locale: Locale) -> Self {
        let catalog = Catalog::new(locale);
        Self {
            title: catalog.localized(Message::Title),
            label: catalog.localized(Message::Label),
            browser: catalog.localized(Message::BrowserDefault),
            help: catalog.localized(Message::BrowserHelp),
            coverage: catalog.localized(Message::Coverage),
            save: catalog.localized(Message::Save),
            saved: catalog.localized(Message::Saved),
            invalid: catalog.localized(Message::Invalid),
            device_only: catalog.localized(Message::DeviceOnly),
        }
    }
}

#[cfg(test)]
#[path = "language_tests.rs"]
mod tests;
