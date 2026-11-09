//! Lazy shared chrome labels: only rendered controls request their catalog text. Templates
//! show each through `ui::localized`, which marks an untranslated label `lang="en"`.
use rbit_localization::{Catalog, ChromeDisplay, ChromeMessage as Message, ChromeSentence, Locale, Localized, Rich};

#[derive(Clone, Copy, Debug)]
pub struct PageChrome {
    catalog: Catalog,
}
impl PageChrome {
    pub const fn new(locale: Locale) -> Self {
        Self { catalog: Catalog::new(locale) }
    }

    pub fn skip(&self) -> Localized {
        self.catalog.localized(Message::Skip)
    }

    pub fn main(&self) -> Localized {
        self.catalog.localized(Message::Main)
    }

    pub fn explore(&self) -> Localized {
        self.catalog.localized(Message::Explore)
    }

    pub fn language(&self) -> Localized {
        self.catalog.localized(Message::Language)
    }

    pub fn switch_theme(&self) -> Localized {
        self.catalog.localized(Message::SwitchTheme)
    }

    pub fn choose_theme(&self) -> Localized {
        self.catalog.localized(Message::ChooseTheme)
    }

    pub fn notifications(&self) -> Localized {
        self.catalog.localized(Message::Notifications)
    }

    pub fn unread(&self) -> Localized {
        self.catalog.localized(Message::Unread)
    }

    pub fn new_repository(&self) -> Localized {
        self.catalog.localized(Message::NewRepository)
    }

    pub fn account_menu(&self) -> Localized {
        self.catalog.localized(Message::AccountMenu)
    }

    pub fn issues(&self) -> Localized {
        self.catalog.localized(Message::Issues)
    }

    pub fn pulls(&self) -> Localized {
        self.catalog.localized(Message::Pulls)
    }

    pub fn dashboard(&self) -> Localized {
        self.catalog.localized(Message::Dashboard)
    }

    pub fn profile(&self) -> Localized {
        self.catalog.localized(Message::Profile)
    }

    pub fn new_organization(&self) -> Localized {
        self.catalog.localized(Message::NewOrganization)
    }

    pub fn settings(&self) -> Localized {
        self.catalog.localized(Message::Settings)
    }

    pub fn administration(&self) -> Localized {
        self.catalog.localized(Message::Administration)
    }

    pub fn sign_out(&self) -> Localized {
        self.catalog.localized(Message::SignOut)
    }

    pub fn sign_in(&self) -> Localized {
        self.catalog.localized(Message::SignIn)
    }

    pub fn sign_up(&self) -> Localized {
        self.catalog.localized(Message::SignUp)
    }

    pub fn menu(&self) -> Localized {
        self.catalog.localized(Message::Menu)
    }

    pub fn create_account(&self) -> Localized {
        self.catalog.localized(Message::CreateAccount)
    }

    pub fn themes(&self) -> Localized {
        self.catalog.localized(Message::Themes)
    }

    pub fn light_mode(&self) -> Localized {
        self.catalog.localized(Message::LightMode)
    }

    pub fn dark_mode(&self) -> Localized {
        self.catalog.localized(Message::DarkMode)
    }

    /// The footer credit; slot 0 is the product name.
    pub fn powered_by(&self) -> Rich {
        ChromeSentence::PoweredBy.rich(self.catalog.locale())
    }

    pub fn update_available(&self) -> Localized {
        self.catalog.localized(Message::UpdateAvailable)
    }

    pub fn timing_help(&self) -> Localized {
        self.catalog.localized(Message::TimingHelp)
    }

    pub fn rendered_in(&self) -> Localized {
        self.catalog.localized(Message::RenderedIn)
    }

    pub fn footer(&self) -> Localized {
        self.catalog.localized(Message::Footer)
    }

    pub fn support(&self) -> Localized {
        self.catalog.localized(Message::Support)
    }

    pub fn terms(&self) -> Localized {
        self.catalog.localized(Message::Terms)
    }

    pub fn privacy(&self) -> Localized {
        self.catalog.localized(Message::Privacy)
    }

    pub fn status(&self) -> Localized {
        self.catalog.localized(Message::Status)
    }

    pub fn home<'a>(&self, site: &'a str) -> ChromeDisplay<'a> {
        self.catalog.chrome_display(Message::Home { site })
    }
}
