//! Lazy shared chrome labels: only rendered controls request their static catalog text.
use rbit_localization::{Catalog, ChromeDisplay, ChromeMessage as Message, Locale, LocalizedText};

#[derive(Clone, Copy, Debug)]
pub struct PageChrome {
    catalog: Catalog,
}
impl PageChrome {
    pub const fn new(locale: Locale) -> Self {
        Self { catalog: Catalog::new(locale) }
    }

    /// `Some("en")` where these labels are the English text of a locale the original chrome
    /// catalog does not cover: the header and footer declare it, the document does not.
    pub const fn lang(&self) -> Option<&'static str> {
        if self.catalog.locale().has_original_catalog() { None } else { Some(Locale::English.tag()) }
    }

    pub fn skip(&self) -> LocalizedText {
        self.catalog.chrome(Message::Skip)
    }

    pub fn main(&self) -> LocalizedText {
        self.catalog.chrome(Message::Main)
    }

    pub fn explore(&self) -> LocalizedText {
        self.catalog.chrome(Message::Explore)
    }

    pub fn switch_theme(&self) -> LocalizedText {
        self.catalog.chrome(Message::SwitchTheme)
    }

    pub fn choose_theme(&self) -> LocalizedText {
        self.catalog.chrome(Message::ChooseTheme)
    }

    pub fn notifications(&self) -> LocalizedText {
        self.catalog.chrome(Message::Notifications)
    }

    pub fn unread(&self) -> LocalizedText {
        self.catalog.chrome(Message::Unread)
    }

    pub fn new_repository(&self) -> LocalizedText {
        self.catalog.chrome(Message::NewRepository)
    }

    pub fn account_menu(&self) -> LocalizedText {
        self.catalog.chrome(Message::AccountMenu)
    }

    pub fn issues(&self) -> LocalizedText {
        self.catalog.chrome(Message::Issues)
    }

    pub fn pulls(&self) -> LocalizedText {
        self.catalog.chrome(Message::Pulls)
    }

    pub fn dashboard(&self) -> LocalizedText {
        self.catalog.chrome(Message::Dashboard)
    }

    pub fn profile(&self) -> LocalizedText {
        self.catalog.chrome(Message::Profile)
    }

    pub fn new_organization(&self) -> LocalizedText {
        self.catalog.chrome(Message::NewOrganization)
    }

    pub fn settings(&self) -> LocalizedText {
        self.catalog.chrome(Message::Settings)
    }

    pub fn administration(&self) -> LocalizedText {
        self.catalog.chrome(Message::Administration)
    }

    pub fn sign_out(&self) -> LocalizedText {
        self.catalog.chrome(Message::SignOut)
    }

    pub fn sign_in(&self) -> LocalizedText {
        self.catalog.chrome(Message::SignIn)
    }

    pub fn sign_up(&self) -> LocalizedText {
        self.catalog.chrome(Message::SignUp)
    }

    pub fn menu(&self) -> LocalizedText {
        self.catalog.chrome(Message::Menu)
    }

    pub fn create_account(&self) -> LocalizedText {
        self.catalog.chrome(Message::CreateAccount)
    }

    pub fn themes(&self) -> LocalizedText {
        self.catalog.chrome(Message::Themes)
    }

    pub fn light_mode(&self) -> LocalizedText {
        self.catalog.chrome(Message::LightMode)
    }

    pub fn dark_mode(&self) -> LocalizedText {
        self.catalog.chrome(Message::DarkMode)
    }

    pub fn powered_by(&self) -> LocalizedText {
        self.catalog.chrome(Message::PoweredBy)
    }

    pub fn update_available(&self) -> LocalizedText {
        self.catalog.chrome(Message::UpdateAvailable)
    }

    pub fn timing_help(&self) -> LocalizedText {
        self.catalog.chrome(Message::TimingHelp)
    }

    pub fn rendered_in(&self) -> LocalizedText {
        self.catalog.chrome(Message::RenderedIn)
    }

    pub fn footer(&self) -> LocalizedText {
        self.catalog.chrome(Message::Footer)
    }

    pub fn support(&self) -> LocalizedText {
        self.catalog.chrome(Message::Support)
    }

    pub fn terms(&self) -> LocalizedText {
        self.catalog.chrome(Message::Terms)
    }

    pub fn privacy(&self) -> LocalizedText {
        self.catalog.chrome(Message::Privacy)
    }

    pub fn status(&self) -> LocalizedText {
        self.catalog.chrome(Message::Status)
    }

    pub fn home<'a>(&self, site: &'a str) -> ChromeDisplay<'a> {
        self.catalog.chrome_display(Message::Home { site })
    }
}
