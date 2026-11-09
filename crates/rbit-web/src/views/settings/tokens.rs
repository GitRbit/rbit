//! The access tokens page: the create form and the list of existing tokens.
use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{AccessToken, Timestamp, TokenScope};
use rbit_localization::{
    Locale, Localized,
    settings::tokens::{TokenText, TokensMessage},
};

use super::{SettingsLink, SettingsSection};
use crate::views::{
    components::FormError,
    layout::Page,
    when::{date, relative_at},
};

/// Expiry preselected in the create form.
const DEFAULT_EXPIRY: Option<u32> = Some(30);
/// Form value of the "No expiration" choice.
const NEVER_EXPIRES: &str = "never";

/// One existing token in the list.
#[derive(Debug, Clone)]
pub struct TokenRow {
    pub name: String,
    /// e.g. `…a1b2`: the last characters of the secret. No prefix: the stored hint does not
    /// record whether a token was issued as `rbit_…` or, before that, as `rit_…`.
    pub hint: String,
    pub scope_label: &'static str,
    /// e.g. `Created Oct 15, 2026`.
    pub created: Localized,
    pub created_iso: String,
    /// e.g. `Last used 3 hours ago` or `Never used`.
    pub last_used: Localized,
    /// e.g. `Expires Nov 14, 2026`, `Expired Oct 1, 2026` or `No expiration`.
    pub expiry: Localized,
    pub is_expired: bool,
    pub revoke_url: String,
}

impl TokenRow {
    /// `now` decides whether the token has expired (passed in so formatting is testable).
    pub fn new(token: &AccessToken, now: Timestamp, locale: Locale) -> Self {
        let is_expired = token.expires_at.is_some_and(|at| at <= now);
        let expiry = match token.expires_at {
            None => TokensMessage::NoExpiration.text(&locale),
            Some(at) if is_expired => TokenText::Expired(date(at, locale).as_str()).text(&locale),
            Some(at) => TokenText::Expires(date(at, locale).as_str()).text(&locale),
        };
        Self {
            name: token.name.clone(),
            hint: format!("…{}", token.hint),
            scope_label: token.scope.label(),
            created: TokenText::CreatedOn(date(token.created_at, locale).as_str()).text(&locale),
            created_iso: token.created_at.iso8601(),
            last_used: token.last_used_at.map_or_else(
                || TokensMessage::NeverUsed.text(&locale),
                |at| TokenText::LastUsed(relative_at(at, now, locale).as_str()).text(&locale),
            ),
            expiry,
            is_expired,
            revoke_url: format!("/settings/tokens/{}/revoke", token.id.0),
        }
    }
}

/// What each scope allows, as the create form explains it.
const fn scope_description(scope: TokenScope) -> TokensMessage {
    match scope {
        TokenScope::Read => TokensMessage::ReadDescription,
        TokenScope::Write => TokensMessage::WriteDescription,
        TokenScope::Admin => TokensMessage::AdminDescription,
    }
}

/// One radio button of the scope picker.
#[derive(Debug, Clone)]
pub struct ScopeChoice {
    pub value: &'static str,
    pub label: &'static str,
    pub description: TokensMessage,
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
    days: Option<u32>,
    pub selected: bool,
}

impl ExpiryChoice {
    /// One option per entry of `choices`, with `selected` preselected.
    pub fn list(choices: &[Option<u32>], selected: Option<u32>) -> Vec<Self> {
        choices.iter().map(|&days| Self { value: expiry_value(days), days, selected: days == selected }).collect()
    }

    /// The option's text in the page's language.
    #[allow(clippy::trivially_copy_pass_by_ref, reason = "Askama passes the page locale by reference")]
    pub fn label(&self, locale: &Locale) -> Localized {
        match self.days {
            None => TokensMessage::NoExpiration.text(locale),
            Some(1) => TokensMessage::OneDay.text(locale),
            Some(days) => TokenText::Days(u64::from(days)).text(locale),
        }
    }
}

fn expiry_value(days: Option<u32>) -> String {
    days.map_or_else(|| NEVER_EXPIRES.to_owned(), |d| d.to_string())
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
        let locale = page.locale;
        Self {
            page: page.titled(TokensMessage::Title.text(&locale).as_str()),
            settings_nav: SettingsSection::AccessTokens.nav(),
            username,
            error: FormError::default(),
            created: None,
            form,
            tokens: tokens.iter().map(|token| TokenRow::new(token, now, locale)).collect(),
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
    const EN: Locale = Locale::English;

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
        let row = TokenRow::new(&token(Some(Timestamp(NOW.0 + 30 * DAY)), None), NOW, EN);
        assert_eq!(row.hint, "…a1b2");
        assert_eq!(row.scope_label, "Write");
        assert_eq!(row.created.as_str(), "Created Oct 5, 2026");
        assert_eq!(row.last_used.as_str(), "Never used");
        assert_eq!(row.expiry.as_str(), "Expires Nov 14, 2026");
        assert!(!row.is_expired);
        assert_eq!(row.revoke_url, "/settings/tokens/42/revoke");
    }

    #[test]
    fn row_marks_expired_and_used_tokens() {
        let row = TokenRow::new(&token(Some(Timestamp(NOW.0 - DAY)), Some(Timestamp::now())), NOW, EN);
        assert!(row.is_expired);
        assert_eq!(row.expiry.as_str(), "Expired Oct 14, 2026");
        assert_eq!(row.last_used.as_str(), "Last used just now");

        let row = TokenRow::new(&token(None, None), NOW, EN);
        assert_eq!(row.expiry.as_str(), "No expiration");
        assert!(!row.is_expired);
    }

    #[test]
    fn row_text_is_translated_in_spanish() {
        let row = TokenRow::new(&token(None, None), NOW, Locale::Spanish);
        assert_eq!(row.expiry.as_str(), "Sin caducidad");
        assert!(!row.expiry.fallback());
    }

    #[test]
    fn expiry_choices_round_trip() {
        let options = ExpiryChoice::list(CHOICES, Some(30));
        let labels: Vec<_> = options.iter().map(|o| o.label(&EN).to_string()).collect();
        assert_eq!(labels, ["7 days", "30 days", "90 days", "365 days", "No expiration"]);
        assert_eq!(ExpiryChoice::list(&[Some(1)], None)[0].label(&EN).as_str(), "1 day");
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
}
