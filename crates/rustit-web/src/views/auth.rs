use askama::Template;
use askama_web::WebTemplate;

use crate::views::{components::FormError, layout::Page};

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
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "auth/signup.html")]
pub struct SignupPage {
    pub page: Page,
    pub error: FormError,
    pub username: String,
    pub email: String,
    pub min_password_length: usize,
    pub registration_open: bool,
}

/// The second sign-in step for accounts with two-factor authentication.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "auth/two_factor.html")]
pub struct TwoFactorLoginPage {
    pub page: Page,
    pub error: FormError,
    /// Where to go after signing in.
    pub next: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Page {
        Page {
            site_name: "Rustit".to_owned(),
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
        }
        .render()
        .unwrap();
        assert!(html.contains("Too many sign-in attempts. Try again in 2 minutes."));
        assert!(html.contains(r#"value="alice""#));
    }

    #[test]
    fn two_factor_page_asks_for_a_code() {
        let html = TwoFactorLoginPage { page: page(), error: FormError::new("That code did not work."), next: "/new".to_owned() }
            .render()
            .unwrap();
        assert!(html.contains(r#"action="/login/two-factor""#));
        assert!(html.contains(r#"name="code""#));
        assert!(html.contains(r#"autocomplete="one-time-code""#));
        assert!(html.contains(r#"value="/new""#));
        assert!(html.contains("That code did not work."));
        assert!(html.contains("recovery codes"));
    }
}
