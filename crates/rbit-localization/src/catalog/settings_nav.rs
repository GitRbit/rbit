//! Account settings navigation. Spanish reuses the shared shell's existing entries for
//! the labels it already translates; the other sections fall back to English.
use super::fallback::{MessageFamily, MessageWriter};
use super::{Catalog, ChromeMessage};
use crate::Locale;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsNavMessage {
    Settings,
    Profile,
    Account,
    Appearance,
    Language,
    Security,
    SshKeys,
    AccessTokens,
    Notifications,
    Actions,
}

impl SettingsNavMessage {
    /// The shell message with the same meaning, where one exists.
    const fn shell(self) -> Option<ChromeMessage<'static>> {
        match self {
            Self::Settings => Some(ChromeMessage::Settings),
            Self::Profile => Some(ChromeMessage::Profile),
            Self::Notifications => Some(ChromeMessage::Notifications),
            _ => None,
        }
    }
}

impl MessageFamily for SettingsNavMessage {
    fn id(self) -> &'static str {
        match self {
            Self::Settings => "settings.nav.title",
            Self::Profile => "settings.nav.profile",
            Self::Account => "settings.nav.account",
            Self::Appearance => "settings.nav.appearance",
            Self::Language => "settings.nav.language",
            Self::Security => "settings.nav.security",
            Self::SshKeys => "settings.nav.ssh-keys",
            Self::AccessTokens => "settings.nav.access-tokens",
            Self::Notifications => "settings.nav.notifications",
            Self::Actions => "settings.nav.actions",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        text.literal(match self {
            Self::Settings => "Settings",
            Self::Profile => "Profile",
            Self::Account => "Account",
            Self::Appearance => "Appearance",
            Self::Language => "Language",
            Self::Security => "Security",
            Self::SshKeys => "SSH keys",
            Self::AccessTokens => "Access tokens",
            Self::Notifications => "Notifications",
            Self::Actions => "Actions",
        });
    }

    fn translate(self, locale: Locale, text: &mut MessageWriter) -> bool {
        // The shell catalog has Spanish entries for some sections; the tables have the rest.
        if let (Locale::Spanish, Some(shell)) = (locale, self.shell()) {
            text.literal(Catalog::new(locale).localized(shell).as_str());
            return true;
        }
        crate::catalog::translations::write(self, locale, text)
    }
}
