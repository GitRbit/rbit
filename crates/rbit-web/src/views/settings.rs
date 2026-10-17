//! Account settings pages and their view models.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{AccessToken, Timestamp, TokenScope, User};
use rbit_services::profiles::Profile;

use crate::views::{
    components::{Avatar, FormError},
    layout::Page,
};

/// Prefix every token secret starts with; shown before the hint so users recognise it.
const TOKEN_PREFIX: &str = "rit_";
/// Expiry preselected in the create form.
const DEFAULT_EXPIRY: Option<u32> = Some(30);
/// Form value of the "No expiration" choice.
const NEVER_EXPIRES: &str = "never";

// ------------------------------------------------------------------ settings shell

/// A settings area, listed in the left-hand navigation of `settings/layout.html`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Profile,
    Account,
    Appearance,
    Security,
    SshKeys,
    AccessTokens,
}

impl SettingsSection {
    /// Navigation order. New sections slot in here.
    const ALL: [Self; 6] = [Self::Profile, Self::Account, Self::Appearance, Self::Security, Self::SshKeys, Self::AccessTokens];

    const fn label(self) -> &'static str {
        match self {
            Self::Profile => "Profile",
            Self::Account => "Account",
            Self::Appearance => "Appearance",
            Self::Security => "Security",
            Self::SshKeys => "SSH keys",
            Self::AccessTokens => "Access tokens",
        }
    }

    /// Where the section lives; also the redirect target after a successful form post.
    pub const fn url(self) -> &'static str {
        match self {
            Self::Profile => "/settings/profile",
            Self::Account => "/settings/account",
            Self::Appearance => "/settings/appearance",
            Self::Security => "/settings/security",
            Self::SshKeys => "/settings/keys",
            Self::AccessTokens => "/settings/tokens",
        }
    }

    /// The navigation with `self` marked as the current page.
    pub fn nav(self) -> Vec<SettingsLink> {
        Self::ALL.iter().map(|&section| SettingsLink { label: section.label(), url: section.url(), is_active: section == self }).collect()
    }
}

#[derive(Debug, Clone)]
pub struct SettingsLink {
    pub label: &'static str,
    pub url: &'static str,
    pub is_active: bool,
}

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
            page: page.titled("Profile"),
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
    Email,
    Password,
    /// "Resend verification email".
    Verification,
}

impl AccountForm {
    /// The `?saved=` value used after a successful change.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Password => "password",
            Self::Verification => "verification",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
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
    pub page: Page,
    pub settings_nav: Vec<SettingsLink>,
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
            page: page.titled("Account"),
            settings_nav: SettingsSection::Account.nav(),
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

    /// Re-renders with `error` shown above `form`; the email field keeps `submitted_email`.
    #[must_use]
    pub fn with_error(mut self, form: AccountForm, error: FormError, submitted_email: Option<String>) -> Self {
        match form {
            AccountForm::Email => self.email_error = error,
            AccountForm::Password => self.password_error = error,
            AccountForm::Verification => self.verification_error = error,
        }
        if let Some(email) = submitted_email {
            self.email = email;
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
}

// ------------------------------------------------------------------ access tokens

/// One existing token in the list.
#[derive(Debug, Clone)]
pub struct TokenRow {
    pub name: String,
    /// e.g. `rit_…a1b2`.
    pub hint: String,
    pub scope_label: &'static str,
    /// e.g. `Created Oct 15, 2026`.
    pub created: String,
    pub created_iso: String,
    /// e.g. `Last used 3 hours ago` or `Never used`.
    pub last_used: String,
    /// e.g. `Expires Nov 14, 2026`, `Expired Oct 1, 2026` or `No expiration`.
    pub expiry: String,
    pub is_expired: bool,
    pub revoke_url: String,
}

impl TokenRow {
    /// `now` decides whether the token has expired (passed in so formatting is testable).
    pub fn new(token: &AccessToken, now: Timestamp) -> Self {
        let is_expired = token.expires_at.is_some_and(|at| at <= now);
        let expiry = match token.expires_at {
            None => "No expiration".to_owned(),
            Some(at) if is_expired => format!("Expired {}", at.date()),
            Some(at) => format!("Expires {}", at.date()),
        };
        Self {
            name: token.name.clone(),
            hint: format!("{TOKEN_PREFIX}…{}", token.hint),
            scope_label: scope_label(token.scope),
            created: format!("Created {}", token.created_at.date()),
            created_iso: token.created_at.iso8601(),
            last_used: token.last_used_at.map_or_else(|| "Never used".to_owned(), |at| format!("Last used {}", at.relative())),
            expiry,
            is_expired,
            revoke_url: format!("/settings/tokens/{}/revoke", token.id.0),
        }
    }
}

const fn scope_label(scope: TokenScope) -> &'static str {
    scope.label()
}

/// What each scope allows, as the create form explains it.
const fn scope_description(scope: TokenScope) -> &'static str {
    match scope {
        TokenScope::Read => "Clone and fetch; read through the API.",
        TokenScope::Write => "Also push, create repositories and star through the API.",
        TokenScope::Admin => "Also change repository settings and webhooks, and delete repositories through the API.",
    }
}

/// One radio button of the scope picker.
#[derive(Debug, Clone)]
pub struct ScopeChoice {
    pub value: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub selected: bool,
}

impl ScopeChoice {
    /// Every scope, weakest first, with `selected` preselected.
    pub fn list(selected: TokenScope) -> Vec<Self> {
        TokenScope::ALL
            .into_iter()
            .map(|scope| Self {
                value: scope.as_str(),
                label: scope.label(),
                description: scope_description(scope),
                selected: scope == selected,
            })
            .collect()
    }
}

/// A freshly created token. Rendered once and never stored.
#[derive(Debug, Clone)]
pub struct CreatedToken {
    pub name: String,
    pub secret: String,
}

/// One `<option>` of the expiration select.
#[derive(Debug, Clone)]
pub struct ExpiryChoice {
    pub value: String,
    pub label: String,
    pub selected: bool,
}

impl ExpiryChoice {
    /// One option per entry of `choices`, with `selected` preselected.
    pub fn list(choices: &[Option<u32>], selected: Option<u32>) -> Vec<Self> {
        choices.iter().map(|&days| Self { value: expiry_value(days), label: expiry_label(days), selected: days == selected }).collect()
    }
}

fn expiry_value(days: Option<u32>) -> String {
    days.map_or_else(|| NEVER_EXPIRES.to_owned(), |d| d.to_string())
}

fn expiry_label(days: Option<u32>) -> String {
    match days {
        None => "No expiration".to_owned(),
        Some(1) => "1 day".to_owned(),
        Some(d) => format!("{d} days"),
    }
}

/// Parses the expiration `<select>` value: `Some(None)` is "never", `None` is not one of `choices`.
#[allow(clippy::option_option)] // "no valid choice" and "never expires" are genuinely different.
pub fn parse_expiry(value: &str, choices: &[Option<u32>]) -> Option<Option<u32>> {
    let days = if value == NEVER_EXPIRES { None } else { Some(value.parse::<u32>().ok()?) };
    choices.contains(&days).then_some(days)
}

/// The create form's current values (blank, or as submitted when re-rendered with an error).
#[derive(Debug, Clone)]
pub struct TokenForm {
    pub name: String,
    pub scope_choices: Vec<ScopeChoice>,
    pub expiry_choices: Vec<ExpiryChoice>,
}

impl TokenForm {
    pub fn blank(choices: &[Option<u32>]) -> Self {
        Self {
            name: String::new(),
            scope_choices: ScopeChoice::list(TokenScope::default()),
            expiry_choices: ExpiryChoice::list(choices, DEFAULT_EXPIRY),
        }
    }

    pub fn submitted(name: String, scope: TokenScope, expiry: Option<Option<u32>>, choices: &[Option<u32>]) -> Self {
        Self {
            name,
            scope_choices: ScopeChoice::list(scope),
            expiry_choices: ExpiryChoice::list(choices, expiry.unwrap_or(DEFAULT_EXPIRY)),
        }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "settings/tokens.html")]
pub struct TokensPage {
    pub page: Page,
    pub settings_nav: Vec<SettingsLink>,
    pub username: String,
    pub error: FormError,
    pub created: Option<CreatedToken>,
    pub form: TokenForm,
    pub tokens: Vec<TokenRow>,
}

impl TokensPage {
    pub fn new(page: Page, username: String, tokens: &[AccessToken], form: TokenForm) -> Self {
        let now = Timestamp::now();
        Self {
            page: page.titled("Access tokens"),
            settings_nav: SettingsSection::AccessTokens.nav(),
            username,
            error: FormError::default(),
            created: None,
            form,
            tokens: tokens.iter().map(|token| TokenRow::new(token, now)).collect(),
        }
    }

    #[must_use]
    pub fn with_error(mut self, error: FormError) -> Self {
        self.error = error;
        self
    }

    #[must_use]
    pub fn with_created(mut self, created: CreatedToken) -> Self {
        self.created = Some(created);
        self
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{AccessTokenId, AccountId};

    use super::*;

    const CHOICES: &[Option<u32>] = &[Some(7), Some(30), Some(90), Some(365), None];
    // 2026-10-15T00:00:00Z
    const NOW: Timestamp = Timestamp(1_792_022_400);
    const DAY: i64 = 86_400;

    fn token(expires_at: Option<Timestamp>, last_used_at: Option<Timestamp>) -> AccessToken {
        AccessToken {
            id: AccessTokenId(42),
            account_id: AccountId(1),
            name: "laptop".to_owned(),
            hint: "a1b2".to_owned(),
            scope: TokenScope::Write,
            created_at: Timestamp(NOW.0 - 10 * DAY),
            last_used_at,
            expires_at,
        }
    }

    #[test]
    fn row_formats_metadata() {
        let row = TokenRow::new(&token(Some(Timestamp(NOW.0 + 30 * DAY)), None), NOW);
        assert_eq!(row.hint, "rit_…a1b2");
        assert_eq!(row.scope_label, "Write");
        assert_eq!(row.created, "Created Oct 5, 2026");
        assert_eq!(row.last_used, "Never used");
        assert_eq!(row.expiry, "Expires Nov 14, 2026");
        assert!(!row.is_expired);
        assert_eq!(row.revoke_url, "/settings/tokens/42/revoke");
    }

    #[test]
    fn row_marks_expired_and_used_tokens() {
        let row = TokenRow::new(&token(Some(Timestamp(NOW.0 - DAY)), Some(Timestamp::now())), NOW);
        assert!(row.is_expired);
        assert_eq!(row.expiry, "Expired Oct 14, 2026");
        assert_eq!(row.last_used, "Last used just now");

        let row = TokenRow::new(&token(None, None), NOW);
        assert_eq!(row.expiry, "No expiration");
        assert!(!row.is_expired);
    }

    #[test]
    fn expiry_choices_round_trip() {
        let options = ExpiryChoice::list(CHOICES, Some(30));
        let labels: Vec<_> = options.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(labels, ["7 days", "30 days", "90 days", "365 days", "No expiration"]);
        assert_eq!(options.iter().filter(|o| o.selected).count(), 1);
        assert!(options[1].selected);
        for option in &options {
            assert!(parse_expiry(&option.value, CHOICES).is_some(), "{} should parse", option.value);
        }
        assert_eq!(parse_expiry("never", CHOICES), Some(None));
        assert_eq!(parse_expiry("90", CHOICES), Some(Some(90)));
        assert_eq!(parse_expiry("12", CHOICES), None);
        assert_eq!(parse_expiry("soon", CHOICES), None);
    }

    #[test]
    fn scope_choices_offer_every_scope_once() {
        let blank = TokenForm::blank(CHOICES);
        let values: Vec<_> = blank.scope_choices.iter().map(|choice| choice.value).collect();
        assert_eq!(values, ["read", "write", "admin"]);
        let selected: Vec<_> = blank.scope_choices.iter().filter(|choice| choice.selected).map(|choice| choice.value).collect();
        assert_eq!(selected, ["read"], "read is the default");

        let submitted = TokenForm::submitted("ops".to_owned(), TokenScope::Admin, None, CHOICES);
        let selected: Vec<_> = submitted.scope_choices.iter().filter(|choice| choice.selected).map(|choice| choice.label).collect();
        assert_eq!(selected, ["Admin"]);
        for choice in &submitted.scope_choices {
            assert_eq!(TokenScope::parse(choice.value).map(TokenScope::label), Some(choice.label));
        }
    }

    #[test]
    fn settings_nav_marks_active_section() {
        let nav = SettingsSection::AccessTokens.nav();
        let labels: Vec<_> = nav.iter().map(|link| link.label).collect();
        assert_eq!(labels, ["Profile", "Account", "Appearance", "Security", "SSH keys", "Access tokens"]);
        assert_eq!(nav.iter().filter(|link| link.is_active).count(), 1);
        let active = nav.iter().find(|link| link.is_active).unwrap();
        assert_eq!(active.url, "/settings/tokens");
        assert!(SettingsSection::Profile.nav()[0].is_active);
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
        Page {
            site_name: "Rbit".to_owned(),
            title: None,
            viewer: None,
            path: "/settings/account".to_owned(),
            theme: crate::themes::Theme::System,
            has_unread_notifications: false,
        }
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
        for form in [AccountForm::Email, AccountForm::Password, AccountForm::Verification] {
            assert_eq!(AccountForm::parse(form.key()), Some(form));
        }
        assert_eq!(AccountForm::parse("tokens"), None);
    }
}
