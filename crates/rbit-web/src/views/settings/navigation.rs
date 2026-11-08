//! Settings sections and native navigation, independent of feature forms.
use rbit_localization::{Catalog, Locale, Localized, SettingsNavMessage};

// ------------------------------------------------------------------ settings shell

/// A settings area, listed in the left-hand navigation of `settings/layout.html`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Profile,
    Account,
    Appearance,
    Language,
    Security,
    SshKeys,
    AccessTokens,
    // Milestone 3: email notifications (N).
    Notifications,
    // Actions: interface and secrets (U).
    /// Only on an instance with Actions: see [`SettingsLink::needs_actions`].
    Actions,
}

impl SettingsSection {
    /// Navigation order. New sections slot in here.
    const ALL: [Self; 9] = [
        Self::Profile,
        Self::Account,
        Self::Appearance,
        Self::Language,
        Self::Security,
        Self::SshKeys,
        Self::AccessTokens,
        Self::Notifications,
        Self::Actions,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Profile => "Profile",
            Self::Account => "Account",
            Self::Appearance => "Appearance",
            Self::Language => "Language",
            Self::Security => "Security",
            Self::SshKeys => "SSH keys",
            Self::AccessTokens => "Access tokens",
            Self::Notifications => "Notifications",
            Self::Actions => "Actions",
        }
    }

    const fn message(self) -> SettingsNavMessage {
        match self {
            Self::Profile => SettingsNavMessage::Profile,
            Self::Account => SettingsNavMessage::Account,
            Self::Appearance => SettingsNavMessage::Appearance,
            Self::Language => SettingsNavMessage::Language,
            Self::Security => SettingsNavMessage::Security,
            Self::SshKeys => SettingsNavMessage::SshKeys,
            Self::AccessTokens => SettingsNavMessage::AccessTokens,
            Self::Notifications => SettingsNavMessage::Notifications,
            Self::Actions => SettingsNavMessage::Actions,
        }
    }

    /// Where the section lives; also the redirect target after a successful form post.
    pub const fn url(self) -> &'static str {
        match self {
            Self::Profile => "/settings/profile",
            Self::Account => "/settings/account",
            Self::Appearance => "/settings/appearance",
            Self::Language => "/settings/language",
            Self::Security => "/settings/security",
            Self::SshKeys => "/settings/keys",
            Self::AccessTokens => "/settings/tokens",
            Self::Notifications => rbit_services::notification_email::SETTINGS_PATH,
            Self::Actions => "/settings/actions",
        }
    }

    /// The navigation with `self` marked as the current page.
    pub fn nav(self) -> Vec<SettingsLink> {
        Self::ALL
            .iter()
            .map(|&section| SettingsLink {
                label: section.label(),
                message: section.message(),
                url: section.url(),
                is_active: section == self,
                needs_actions: section == Self::Actions,
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct SettingsLink {
    /// English label, for tests and logs; pages show [`SettingsLink::text`].
    pub label: &'static str,
    message: SettingsNavMessage,
    pub url: &'static str,
    pub is_active: bool,
    // Actions: interface and secrets (U).
    /// Shown only on an instance with Actions ([`crate::views::layout::Page::has_actions`]): the navigation is
    /// built without the request, and `settings/layout.html` leaves the link out otherwise.
    pub needs_actions: bool,
}

impl SettingsLink {
    /// The label in the page's language, or its English fallback.
    #[allow(clippy::trivially_copy_pass_by_ref, reason = "Askama passes the page locale by reference")]
    pub fn text(&self, locale: &Locale) -> Localized {
        Catalog::new(*locale).localized(self.message)
    }
}

/// The settings navigation's heading in the page's language.
#[allow(clippy::trivially_copy_pass_by_ref, reason = "Askama passes the page locale by reference")]
pub fn title(locale: &Locale) -> Localized {
    Catalog::new(*locale).localized(SettingsNavMessage::Settings)
}
