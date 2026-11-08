//! Account settings pages and their view models.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::User;
use rbit_localization::{
    Localized,
    settings::{
        account::{AccountMessage, AccountText},
        profile::ProfileMessage,
    },
};
use rbit_services::profiles::Profile;

use crate::views::{
    components::{Avatar, FormError},
    layout::Page,
};

mod navigation;
pub use navigation::{SettingsLink, SettingsSection, title as navigation_title};
mod language;
pub mod tokens;
pub use language::LanguagePage;

// ------------------------------------------------------------------ profile

/// The profile form's values (stored, or as submitted when re-rendered with an error).
#[derive(Debug, Clone, Default)]
pub struct ProfileForm {
    pub display_name: String,
    pub bio: String,
    pub website: String,
    pub location: String,
}

impl From<Profile> for ProfileForm {
    fn from(profile: Profile) -> Self {
        Self {
            display_name: profile.display_name.unwrap_or_default(),
            bio: profile.bio.unwrap_or_default(),
            website: profile.website.unwrap_or_default(),
            location: profile.location.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "settings/profile.html")]
pub struct ProfilePage {
    pub page: Page,
    pub settings_nav: Vec<SettingsLink>,
    pub username: String,
    pub avatar: Avatar,
    pub form: ProfileForm,
    pub error: FormError,
    /// Set after a successful save (`?saved=1`).
    pub saved: bool,
}

impl ProfilePage {
    pub fn new(page: Page, user: &User, form: ProfileForm) -> Self {
        Self {
            page: {
                let title = ProfileMessage::Title.text(&page.locale);
                page.titled(title.as_str())
            },
            settings_nav: SettingsSection::Profile.nav(),
            username: user.name().to_string(),
            avatar: Avatar::for_account(&user.account),
            form,
            error: FormError::default(),
            saved: false,
        }
    }

    #[must_use]
    pub fn with_error(mut self, error: FormError) -> Self {
        self.error = error;
        self
    }

    #[must_use]
    pub fn saved(mut self, saved: bool) -> Self {
        self.saved = saved;
        self
    }
}

// ------------------------------------------------------------------ account

/// Which account form a success notice or validation error belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountForm {
    Username,
    Email,
    Password,
    /// "Resend verification email".
    Verification,
}

impl AccountForm {
    /// The `?saved=` value used after a successful change.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Username => "username",
            Self::Email => "email",
            Self::Password => "password",
            Self::Verification => "verification",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "username" => Some(Self::Username),
            "email" => Some(Self::Email),
            "password" => Some(Self::Password),
            "verification" => Some(Self::Verification),
            _ => None,
        }
    }
}

/// Whether the current address is verified, and whether a link can be (re)sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmailVerification {
    Verified,
    /// Not verified; a new link can be requested.
    Pending,
    /// Not verified, and the server can't send email.
    #[default]
    Unavailable,
}

impl EmailVerification {
    pub const fn new(verified: bool, email_available: bool) -> Self {
        match (verified, email_available) {
            (true, _) => Self::Verified,
            (false, true) => Self::Pending,
            (false, false) => Self::Unavailable,
        }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "settings/account.html")]
pub struct AccountPage {
    pub username: String,
    pub username_error: FormError,
    pub page: Page,
    pub settings_nav: Vec<SettingsLink>,
    pub password_change_required: bool,
    pub current_email: String,
    /// The email field's value (the current address, or what was submitted).
    pub email: String,
    pub email_error: FormError,
    pub password_error: FormError,
    pub verification_error: FormError,
    pub verification: EmailVerification,
    pub min_password_length: usize,
    /// The form whose change just succeeded, for the confirmation notice.
    pub saved: Option<AccountForm>,
}

impl AccountPage {
    pub fn new(page: Page, user: &User, min_password_length: usize) -> Self {
        Self {
            page: {
                let title = AccountMessage::Title.text(&page.locale);
                page.titled(title.as_str())
            },
            username: user.name().to_string(),
            username_error: FormError::default(),
            settings_nav: SettingsSection::Account.nav(),
            password_change_required: false,
            current_email: user.email.as_str().to_owned(),
            email: user.email.as_str().to_owned(),
            email_error: FormError::default(),
            password_error: FormError::default(),
            verification_error: FormError::default(),
            verification: EmailVerification::default(),
            min_password_length,
            saved: None,
        }
    }

    #[must_use]
    pub fn verification(mut self, verification: EmailVerification) -> Self {
        self.verification = verification;
        self
    }

    /// Re-renders with `error` above `form`, preserving a submitted username or email.
    #[must_use]
    pub fn with_error(mut self, form: AccountForm, error: FormError, submitted_value: Option<String>) -> Self {
        match form {
            AccountForm::Username => self.username_error = error,
            AccountForm::Email => self.email_error = error,
            AccountForm::Password => self.password_error = error,
            AccountForm::Verification => self.verification_error = error,
        }
        if let Some(value) = submitted_value {
            if form == AccountForm::Username {
                self.username = value;
            } else {
                self.email = value;
            }
        }
        self
    }

    #[must_use]
    pub fn saved(mut self, saved: Option<AccountForm>) -> Self {
        self.saved = saved;
        self
    }

    pub fn saved_email(&self) -> bool {
        self.saved == Some(AccountForm::Email)
    }

    pub fn saved_username(&self) -> bool {
        self.saved == Some(AccountForm::Username)
    }

    pub fn saved_password(&self) -> bool {
        self.saved == Some(AccountForm::Password)
    }

    pub fn sent_verification(&self) -> bool {
        self.saved == Some(AccountForm::Verification)
    }

    pub fn is_verified(&self) -> bool {
        self.verification == EmailVerification::Verified
    }

    pub fn can_resend(&self) -> bool {
        self.verification == EmailVerification::Pending
    }

    /// The new password's length hint.
    pub fn password_hint(&self) -> Localized {
        AccountText::PasswordHint(u64::try_from(self.min_password_length).unwrap_or(u64::MAX)).text(&self.page.locale)
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{AccountId, Timestamp};

    use super::*;

    // 2026-10-15T00:00:00Z
    const NOW: Timestamp = Timestamp(1_792_022_400);

    #[test]
    fn settings_nav_marks_active_section() {
        let nav = SettingsSection::AccessTokens.nav();
        let labels: Vec<_> = nav.iter().map(|link| link.label).collect();
        assert_eq!(
            labels,
            ["Profile", "Account", "Appearance", "Language", "Security", "SSH keys", "Access tokens", "Notifications", "Actions"]
        );
        let only_with_actions: Vec<_> = nav.iter().filter(|link| link.needs_actions).map(|link| link.url).collect();
        assert_eq!(only_with_actions, ["/settings/actions"]);
        assert_eq!(nav.iter().filter(|link| link.is_active).count(), 1);
        let active = nav.iter().find(|link| link.is_active).unwrap();
        assert_eq!(active.url, "/settings/tokens");
        assert!(SettingsSection::Profile.nav()[0].is_active);
        let language = SettingsSection::Language.nav();
        let active = language.iter().find(|link| link.is_active).unwrap();
        assert_eq!((active.label, active.url, active.needs_actions), ("Language", "/settings/language", false));
    }

    fn user() -> User {
        User {
            account: rbit_core::Account {
                id: AccountId(1),
                kind: rbit_core::AccountKind::User,
                name: "alice".parse().unwrap(),
                display_name: None,
                bio: None,
                created_at: NOW,
            },
            email: "alice@example.com".parse().unwrap(),
            is_admin: false,
        }
    }

    fn page() -> Page {
        crate::views::test_support::page("/settings/account")
    }

    #[test]
    fn account_page_shows_error_on_the_right_form() {
        let html = AccountPage::new(page(), &user(), 10)
            .with_error(AccountForm::Email, FormError::new("Email is already taken."), Some("bob@example.com".to_owned()))
            .render()
            .unwrap();
        assert!(html.contains("Email is already taken."));
        assert!(html.contains(r#"value="bob@example.com""#));
        assert!(html.contains(r#"minlength="10""#));
        assert!(!html.contains("Password changed."));

        let html = AccountPage::new(page(), &user(), 10).saved(Some(AccountForm::Password)).render().unwrap();
        assert!(html.contains("Password changed."));

        let html = AccountPage::new(page(), &user(), 10)
            .with_error(AccountForm::Username, FormError::new("Username is already taken."), Some("submitted".into()))
            .render()
            .unwrap();
        assert!(html.contains("Username is already taken."));
        assert!(html.contains(r#"value="submitted""#));
        assert!(html.contains(r#"action="/settings/account/username""#));
        assert!(!html.contains("Username updated."));
    }

    #[test]
    fn profile_page_renders_values() {
        let form = ProfileForm {
            display_name: "Alice".to_owned(),
            bio: "Hi <there>".to_owned(),
            website: String::new(),
            location: "Oxford".to_owned(),
        };
        let html = ProfilePage::new(page(), &user(), form).saved(true).render().unwrap();
        assert!(html.contains("Profile updated."));
        assert!(html.contains("Hi &#60;there&#62;") || html.contains("Hi &lt;there&gt;"), "bio is escaped");
        assert!(html.contains(r#"value="Oxford""#));
    }

    #[test]
    fn account_page_shows_verification_status() {
        let html = AccountPage::new(page(), &user(), 10).verification(EmailVerification::Verified).render().unwrap();
        assert!(html.contains(">Verified<"));
        assert!(!html.contains("/settings/account/verify"));

        let html = AccountPage::new(page(), &user(), 10).verification(EmailVerification::Pending).render().unwrap();
        assert!(html.contains(">Unverified<"));
        assert!(html.contains(r#"action="/settings/account/verify""#));
        assert!(html.contains("Resend verification email"));

        let html = AccountPage::new(page(), &user(), 10)
            .verification(EmailVerification::Pending)
            .saved(Some(AccountForm::Verification))
            .render()
            .unwrap();
        assert!(html.contains("Verification email sent."));

        let html = AccountPage::new(page(), &user(), 10)
            .verification(EmailVerification::Pending)
            .with_error(AccountForm::Verification, FormError::new("Too many attempts."), None)
            .render()
            .unwrap();
        assert!(html.contains("Too many attempts."));

        let html = AccountPage::new(page(), &user(), 10).verification(EmailVerification::Unavailable).render().unwrap();
        assert!(html.contains("Email is not available on this server"));
        assert!(!html.contains("/settings/account/verify"));

        assert_eq!(EmailVerification::new(true, false), EmailVerification::Verified);
        assert_eq!(EmailVerification::new(false, true), EmailVerification::Pending);
        assert_eq!(EmailVerification::new(false, false), EmailVerification::Unavailable);
    }

    #[test]
    fn account_form_keys_round_trip() {
        for form in [AccountForm::Username, AccountForm::Email, AccountForm::Password, AccountForm::Verification] {
            assert_eq!(AccountForm::parse(form.key()), Some(form));
        }
        assert_eq!(AccountForm::parse("tokens"), None);
    }
}
