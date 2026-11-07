//! Native account language preference with static compiled labels.
use super::{SettingsLink, SettingsSection};
use crate::views::layout::Page;
use askama::Template;
use askama_web::WebTemplate;
use rbit_identity_models::AccountLocale;
use rbit_localization::{Catalog, LanguageMessage as Message, LocalizedText};

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
        let title = Catalog::new(locale).language(Message::Title);
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
            title: catalog.language(Message::Title),
            label: catalog.language(Message::Label),
            browser: catalog.language(Message::BrowserDefault),
            help: catalog.language(Message::BrowserHelp),
            coverage: catalog.language(Message::Coverage),
            english: catalog.language(Message::English),
            spanish: catalog.language(Message::Spanish),
            save: catalog.language(Message::Save),
            saved: catalog.language(Message::Saved),
            invalid: catalog.language(Message::Invalid),
        }
    }
}

#[derive(Debug)]
pub struct LanguageText {
    pub title: LocalizedText,
    pub label: LocalizedText,
    pub browser: LocalizedText,
    pub help: LocalizedText,
    pub coverage: LocalizedText,
    pub english: LocalizedText,
    pub spanish: LocalizedText,
    pub save: LocalizedText,
    pub saved: LocalizedText,
    pub invalid: LocalizedText,
}
