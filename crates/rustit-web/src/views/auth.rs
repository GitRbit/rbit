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
