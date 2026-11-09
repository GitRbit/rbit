//! Shared navigation text: the header, the skip link and the footer. Static English labels
//! borrow compiled literals; translations live in `translations/<locale>/chrome.rs`.
use super::Catalog;
use super::collab::rich::{self, Rich, RichFamily, RichWriter};
use super::fallback::{MessageFamily, MessageWriter};
use crate::Locale;
use std::borrow::Borrow;
use std::fmt;

#[derive(Clone, Copy, Debug)]
pub enum ChromeMessage<'a> {
    Skip,
    Main,
    Explore,
    Language,
    SwitchTheme,
    ChooseTheme,
    Notifications,
    Unread,
    NewRepository,
    AccountMenu,
    Issues,
    Pulls,
    Dashboard,
    Profile,
    NewOrganization,
    Settings,
    Administration,
    SignOut,
    SignIn,
    SignUp,
    Menu,
    CreateAccount,
    Themes,
    LightMode,
    DarkMode,
    UpdateAvailable,
    TimingHelp,
    Footer,
    Support,
    Terms,
    Privacy,
    Status,
    Home { site: &'a str },
}
impl ChromeMessage<'_> {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Skip => "chrome.skip",
            Self::Main => "chrome.main",
            Self::Explore => "chrome.explore",
            Self::Language => "chrome.language",
            Self::SwitchTheme => "chrome.switch-theme",
            Self::ChooseTheme => "chrome.choose-theme",
            Self::Notifications => "chrome.notifications",
            Self::Unread => "chrome.unread",
            Self::NewRepository => "chrome.new-repository",
            Self::AccountMenu => "chrome.account-menu",
            Self::Issues => "chrome.issues",
            Self::Pulls => "chrome.pulls",
            Self::Dashboard => "chrome.dashboard",
            Self::Profile => "chrome.profile",
            Self::NewOrganization => "chrome.new-organization",
            Self::Settings => "chrome.settings",
            Self::Administration => "chrome.administration",
            Self::SignOut => "chrome.sign-out",
            Self::SignIn => "chrome.sign-in",
            Self::SignUp => "chrome.sign-up",
            Self::Menu => "chrome.menu",
            Self::CreateAccount => "chrome.create-account",
            Self::Themes => "chrome.themes",
            Self::LightMode => "chrome.light-mode",
            Self::DarkMode => "chrome.dark-mode",
            Self::UpdateAvailable => "chrome.update",
            Self::TimingHelp => "chrome.timing-help",
            Self::Footer => "chrome.footer",
            Self::Support => "chrome.support",
            Self::Terms => "chrome.terms",
            Self::Privacy => "chrome.privacy",
            Self::Status => "chrome.status",
            Self::Home { .. } => "chrome.home",
        }
    }
}
impl ChromeMessage<'_> {
    /// The English text of a fixed label.
    const fn english_label(self) -> Option<&'static str> {
        Some(match self {
            Self::Skip => "Skip to content",
            Self::Main => "Main",
            Self::Explore => "Explore",
            Self::Language => "Language",
            Self::SwitchTheme => "Switch between light and dark",
            Self::ChooseTheme => "Choose a theme",
            Self::Notifications => "Notifications",
            Self::Unread => "You have unread notifications",
            Self::NewRepository => "New repository",
            Self::AccountMenu => "Account menu",
            Self::Issues => "Issues",
            Self::Pulls => "Pull requests",
            Self::Dashboard => "Dashboard",
            Self::Profile => "Profile",
            Self::NewOrganization => "New organization",
            Self::Settings => "Settings",
            Self::Administration => "Site administration",
            Self::SignOut => "Sign out",
            Self::SignIn => "Sign in",
            Self::SignUp => "Sign up",
            Self::Menu => "Menu",
            Self::CreateAccount => "Create an account",
            Self::Themes => "Themes",
            Self::LightMode => "Light mode",
            Self::DarkMode => "Dark mode",
            Self::UpdateAvailable => "Update available",
            Self::TimingHelp => "Server time for the last page or update",
            Self::Footer => "Footer",
            Self::Support => "Support",
            Self::Terms => "Terms",
            Self::Privacy => "Privacy",
            Self::Status => "Status",
            Self::Home { .. } => return None,
        })
    }
}

impl MessageFamily for ChromeMessage<'_> {
    fn id(self) -> &'static str {
        Self::id(self)
    }

    fn english(self, text: &mut MessageWriter) {
        match (self, self.english_label()) {
            (_, Some(label)) => text.literal(label),
            (Self::Home { site }, None) => {
                text.argument(site);
                text.literal(" home");
            }
            (_, None) => {}
        }
    }

    fn english_literal(self) -> Option<&'static str> {
        self.english_label()
    }

    fn translate(self, locale: Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

impl Catalog {
    pub const fn chrome_display(self, message: ChromeMessage<'_>) -> ChromeDisplay<'_> {
        ChromeDisplay { catalog: self, message }
    }
}

/// Dynamic home labels borrow the site name and format directly in real catalogs.
#[derive(Clone, Copy, Debug)]
pub struct ChromeDisplay<'a> {
    catalog: Catalog,
    message: ChromeMessage<'a>,
}
impl fmt::Display for ChromeDisplay<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let ChromeMessage::Home { site } = self.message
            && self.catalog.locale == Locale::English
        {
            return write!(output, "{site} home");
        }
        fmt::Display::fmt(&self.catalog.localized(self.message), output)
    }
}

/// Chrome sentences that embed markup; the template renders each slot.
#[derive(Clone, Copy, Debug)]
pub enum ChromeSentence {
    /// The footer credit. Slot 0: the product name.
    PoweredBy,
    /// The footer's page timing. Slot 0: the duration.
    RenderedIn,
}

impl ChromeSentence {
    /// This sentence in `locale`, or its English text marked as a fallback.
    pub fn rich(self, locale: impl Borrow<Locale>) -> Rich {
        rich::render(*locale.borrow(), self)
    }
}

impl RichFamily for ChromeSentence {
    fn id(self) -> &'static str {
        match self {
            Self::PoweredBy => "chrome.powered-by",
            Self::RenderedIn => "chrome.rendered-in",
        }
    }

    fn english(self, text: &mut RichWriter) {
        match self {
            Self::PoweredBy => {
                text.literal("Powered by ");
                text.slot(0);
            }
            Self::RenderedIn => {
                text.literal("rendered in ");
                text.slot(0);
            }
        }
    }

    fn translate(self, locale: Locale, text: &mut RichWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
