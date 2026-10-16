//! Password reset and email verification pages.
//!
//! Milestone 1 workstream E owns this file. The pages share the sign-in page's layout: a
//! narrow column with the logo, a heading and one form or one message.

use askama::Template;
use askama_web::WebTemplate;

use crate::views::{components::FormError, layout::Page};

/// `/password/forgot`: asks for an address, then confirms a link may have been sent.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "recovery/forgot.html")]
pub struct ForgotPasswordPage {
    pub page: Page,
    pub error: FormError,
    /// Re-filled after a validation error.
    pub email: String,
    /// Set after a request (`?sent=1`). Deliberately says the same whether or not an
    /// account uses the address.
    pub sent: bool,
    /// Without email the page explains that an administrator has to help instead.
    pub email_available: bool,
}

impl ForgotPasswordPage {
    pub fn new(page: Page, email_available: bool) -> Self {
        Self { page: page.titled("Reset your password"), error: FormError::default(), email: String::new(), sent: false, email_available }
    }

    #[must_use]
    pub fn sent(mut self, sent: bool) -> Self {
        self.sent = sent;
        self
    }

    /// Re-renders the form with `error` and the submitted address.
    #[must_use]
    pub fn with_error(mut self, error: FormError, email: String) -> Self {
        self.error = error;
        self.email = email;
        self
    }
}

/// Why a reset link can't be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkProblem {
    /// Unknown, expired or already used (indistinguishable on purpose).
    Invalid,
    /// The account has been suspended.
    Suspended,
}

impl LinkProblem {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Invalid => "This link has expired",
            Self::Suspended => "This account is suspended",
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::Invalid => "Password reset links work once and expire after an hour. Request a new one to continue.",
            Self::Suspended => "Its password can't be reset. Contact an administrator.",
        }
    }

    pub const fn is_invalid(self) -> bool {
        matches!(self, Self::Invalid)
    }
}

/// `/password/reset/{token}`: the new-password form, or why the link can't be used.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "recovery/reset.html")]
pub struct ResetPasswordPage {
    pub page: Page,
    pub error: FormError,
    /// Where the form posts (the link itself).
    pub action: String,
    /// Whose password is being reset; also given to password managers.
    pub username: String,
    pub min_password_length: usize,
    pub problem: Option<LinkProblem>,
    /// Whether a new link can be requested.
    pub email_available: bool,
}

impl ResetPasswordPage {
    pub fn form(page: Page, token: &str, username: String, min_password_length: usize) -> Self {
        Self {
            page: page.titled("Choose a new password"),
            error: FormError::default(),
            action: format!("/password/reset/{token}"),
            username,
            min_password_length,
            problem: None,
            email_available: true,
        }
    }

    pub fn problem(page: Page, problem: LinkProblem, email_available: bool) -> Self {
        Self {
            page: page.titled(problem.title()),
            error: FormError::default(),
            action: String::new(),
            username: String::new(),
            min_password_length: 0,
            problem: Some(problem),
            email_available,
        }
    }

    #[must_use]
    pub fn with_error(mut self, error: FormError) -> Self {
        self.error = error;
        self
    }
}

/// `/password/reset/complete`: shown after a successful reset.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "recovery/reset_complete.html")]
pub struct ResetCompletePage {
    pub page: Page,
}

impl ResetCompletePage {
    pub fn new(page: Page) -> Self {
        Self { page: page.titled("Password changed") }
    }
}

/// `/email/verify/{token}`: the outcome of a verification link.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "recovery/verify.html")]
pub struct VerifyEmailPage {
    pub page: Page,
    /// The confirmed address, or `None` if the link was not valid.
    pub verified_address: Option<String>,
}

impl VerifyEmailPage {
    pub fn verified(page: Page, address: String) -> Self {
        Self { page: page.titled("Email address verified"), verified_address: Some(address) }
    }

    pub fn invalid(page: Page) -> Self {
        Self { page: page.titled("This link has expired"), verified_address: None }
    }

    /// Where to go next: the account settings when signed in, sign-in otherwise.
    pub fn continue_url(&self) -> &'static str {
        if self.page.is_signed_in() { "/settings/account" } else { "/login" }
    }

    pub fn continue_label(&self) -> &'static str {
        if self.page.is_signed_in() { "Go to account settings" } else { "Sign in" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Page {
        Page {
            site_name: "Rustit".to_owned(),
            title: None,
            viewer: None,
            path: "/password/forgot".to_owned(),
            theme: crate::themes::Theme::System,
        }
    }

    #[test]
    fn forgot_page_renders_each_state() {
        let html = ForgotPasswordPage::new(page(), true).render().unwrap();
        assert!(html.contains(r#"action="/password/forgot""#));
        assert!(html.contains(r#"name="email""#));

        let html = ForgotPasswordPage::new(page(), true)
            .with_error(FormError::new("Enter a valid email address."), "nope<".to_owned())
            .render()
            .unwrap();
        assert!(html.contains("Enter a valid email address."));
        assert!(html.contains(r#"value="nope&#60;""#) || html.contains(r#"value="nope&lt;""#));

        let html = ForgotPasswordPage::new(page(), true).sent(true).render().unwrap();
        assert!(html.contains("Check your email"));
        assert!(html.contains("If an account uses that address"));
        assert!(!html.contains(r#"name="email""#));

        let html = ForgotPasswordPage::new(page(), false).render().unwrap();
        assert!(html.contains("Email is not available on this server"));
        assert!(html.contains("administrator"));
        assert!(!html.contains("<form method=\"post\" action=\"/password/forgot\""));
    }

    #[test]
    fn reset_page_renders_the_form_or_the_problem() {
        let html = ResetPasswordPage::form(page(), "tok_EN", "alice".to_owned(), 12).render().unwrap();
        assert!(html.contains(r#"action="/password/reset/tok_EN""#));
        assert!(html.contains(r#"value="alice""#));
        assert!(html.contains(r#"minlength="12""#));
        assert!(html.contains(r#"name="new_password""#) && html.contains(r#"name="confirm_password""#));

        let html = ResetPasswordPage::problem(page(), LinkProblem::Invalid, true).render().unwrap();
        assert!(html.contains("This link has expired"));
        assert!(html.contains(r#"href="/password/forgot""#));
        assert!(!html.contains(r#"name="new_password""#));

        let html = ResetPasswordPage::problem(page(), LinkProblem::Invalid, false).render().unwrap();
        assert!(html.contains("Email is not available on this server"));
        assert!(!html.contains(r#"href="/password/forgot""#));

        let html = ResetPasswordPage::problem(page(), LinkProblem::Suspended, true).render().unwrap();
        assert!(html.contains("suspended"));
    }

    #[test]
    fn complete_and_verify_pages_render() {
        let html = ResetCompletePage::new(page()).render().unwrap();
        assert!(html.contains("Password changed"));
        assert!(html.contains(r#"href="/login""#));

        let html = VerifyEmailPage::verified(page(), "alice@example.com".to_owned()).render().unwrap();
        assert!(html.contains("alice@example.com"));
        assert!(html.contains(r#"href="/login""#));

        let html = VerifyEmailPage::invalid(page()).render().unwrap();
        assert!(html.contains("This link has expired"));
        assert!(html.contains("account settings"));
    }
}
