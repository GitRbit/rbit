//! View models for sign-in, sign-up and two-factor authentication.

use askama::Template;
use askama_web::WebTemplate;

use crate::views::{components::FormError, external_auth::ProviderButton, layout::Page};

#[derive(Debug, Template, WebTemplate)]
#[template(path = "auth/login.html")]
pub struct LoginPage {
    pub page: Page,
    pub error: FormError,
    /// Re-filled after a failed attempt; the password never is.
    pub login: String,
    /// Where to go after signing in.
    pub next: String,
    pub registration_open: bool,
    /// Milestone 3 (O): "Sign in with …" for each configured provider.
    pub providers: Vec<ProviderButton>,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "auth/signup.html")]
pub struct SignupPage {
    pub page: Page,
    pub error: FormError,
    pub username: String,
    pub email: String,
    /// Re-filled after a failed attempt, like the username.
    pub setup_token: String,
    pub min_password_length: usize,
    pub registration_open: bool,
    /// The instance has no user yet: ask for the setup token from the server log.
    pub setup_required: bool,
}

/// The second sign-in step for accounts with two-factor authentication.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "auth/two_factor.html")]
pub struct TwoFactorLoginPage {
    pub page: Page,
    pub error: FormError,
    /// Where to go after signing in.
    pub next: String,
    /// `navigator.credentials.get` options (JSON) if the account has passkeys.
    pub passkey_options: Option<String>,
    /// The account has an authenticator app (otherwise the code field takes recovery
    /// codes only).
    pub authenticator: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Page {
        Page {
            locale: rbit_messages::LocaleSnapshot::english(0),
            ui: Default::default(),
            has_i18n: false,
            updates: rbit_services::updates::UpdateStatus::default(),
            site_name: "rbit".to_owned(),
            plugin_revision: 0,
            branding: rbit_core::config::BrandingConfig::default(),
            title: None,
            viewer: None,
            path: "/login".to_owned(),
            theme: crate::themes::Theme::System,
            has_unread_notifications: false,
        }
    }

    #[test]
    fn login_page_shows_throttling_message() {
        let html = LoginPage {
            page: page(),
            error: FormError::new("Too many sign-in attempts. Try again in 2 minutes."),
            login: "alice".to_owned(),
            next: "/settings".to_owned(),
            registration_open: false,
            providers: Vec::new(),
        }
        .render()
        .unwrap();
        assert!(html.contains("Too many sign-in attempts. Try again in 2 minutes."));
        assert!(html.contains(r#"value="alice""#));
        assert!(!html.contains("Sign in with"));
    }

    #[test]
    fn login_page_offers_each_provider() {
        let providers = vec![ProviderButton { name: "GitHub".to_owned(), url: "/login/github?next=%2Fnew".to_owned() }];
        let html = LoginPage {
            page: page(),
            error: FormError::default(),
            login: String::new(),
            next: "/new".to_owned(),
            registration_open: true,
            providers,
        }
        .render()
        .unwrap();
        assert!(html.contains("Sign in with GitHub"));
        assert!(html.contains(r#"href="/login/github?next=%2Fnew" class="btn btn-lg w-full" hx-boost="false""#));
        assert!(html.contains(r#"action="/login""#), "the password form stays");
    }

    #[test]
    fn two_factor_page_asks_for_a_code() {
        let html = TwoFactorLoginPage {
            page: page(),
            error: FormError::new("That code did not work."),
            next: "/new".to_owned(),
            passkey_options: None,
            authenticator: true,
        }
        .render()
        .unwrap();
        assert!(html.contains(r#"action="/login/two-factor""#));
        assert!(html.contains(r#"name="code""#));
        assert!(html.contains(r#"autocomplete="one-time-code""#));
        assert!(html.contains(r#"value="/new""#));
        assert!(html.contains("That code did not work."));
        assert!(html.contains("recovery codes"));
        assert!(!html.contains("data-passkey-get"));
    }

    #[test]
    fn two_factor_page_offers_a_passkey() {
        let html = TwoFactorLoginPage {
            page: page(),
            error: FormError::default(),
            next: "/new".to_owned(),
            passkey_options: Some(r#"{"challenge":"abc"}"#.to_owned()),
            authenticator: false,
        }
        .render()
        .unwrap();
        assert!(html.contains("data-passkey-get="));
        assert!(html.contains(r#"name="passkey""#));
        assert!(html.contains("Use a passkey"));
        assert!(html.contains("Recovery code"), "without an app, the code field takes recovery codes");
        assert!(!html.contains("authenticator app"));
    }
}
