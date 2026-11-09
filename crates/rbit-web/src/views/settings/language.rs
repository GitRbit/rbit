//! Native account language preference with compiled labels; an untranslated label is
//! marked `lang="en"` by the template.
use super::{SettingsLink, SettingsSection};
use crate::views::layout::Page;
use askama::Template;
use askama_web::WebTemplate;
use rbit_identity_models::AccountLocale;
use rbit_localization::{Catalog, LanguageMessage as Message, Localized};

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
        let catalog = Catalog::new(self.page.locale);
        LanguageText {
            title: catalog.localized(Message::Title),
            label: catalog.localized(Message::Label),
            browser: catalog.localized(Message::BrowserDefault),
            help: catalog.localized(Message::BrowserHelp),
            coverage: catalog.localized(Message::Coverage),
            save: catalog.localized(Message::Save),
            saved: catalog.localized(Message::Saved),
            invalid: catalog.localized(Message::Invalid),
        }
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
}
