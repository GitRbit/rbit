//! Account recovery, verification and address-change notices.

use super::MailMessage;
use crate::MailError;
use askama::Template;

// ------------------------------------------------------------------ password reset

/// Sent from `/password/forgot` (or by an administrator): a link to choose a new password.
#[derive(Clone)]
pub struct PasswordReset {
    pub site_name: String,
    pub base_url: String,
    pub username: String,
    pub link: String,
    /// e.g. `1 hour`.
    pub valid_for: String,
}

impl PasswordReset {
    fn subject(&self) -> String {
        format!("Reset your {} password", self.site_name)
    }
}

#[derive(Template)]
#[template(path = "password_reset.txt")]
struct PasswordResetText<'a> {
    m: &'a PasswordReset,
}

#[derive(Template)]
#[template(path = "password_reset.html")]
struct PasswordResetHtml<'a> {
    m: &'a PasswordReset,
}

templates!(PasswordReset, text = PasswordResetText, html = PasswordResetHtml);

// ------------------------------------------------------------------ verification

/// Why a verification link is being sent; only the opening sentence differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyReason {
    /// The account was just created.
    SignUp,
    /// The address replaced an earlier one.
    Changed,
    /// The user asked for another link.
    Requested,
}

/// Sent after sign-up, after an address change or on request: a link that confirms the address.
#[derive(Clone)]
pub struct VerifyEmail {
    pub site_name: String,
    pub base_url: String,
    pub username: String,
    pub link: String,
    /// e.g. `2 days`.
    pub valid_for: String,
    pub reason: VerifyReason,
}

impl VerifyEmail {
    fn subject(&self) -> String {
        format!("Confirm your email address for {}", self.site_name)
    }

    /// The opening sentence, which leads into the link.
    pub fn introduction(&self) -> String {
        match self.reason {
            VerifyReason::SignUp => format!("Welcome to {}! Confirm that this email address belongs to you:", self.site_name),
            VerifyReason::Changed => {
                format!("You set a new email address for your {} account. Confirm that it belongs to you:", self.site_name)
            }
            VerifyReason::Requested => format!("Confirm that this email address belongs to your {} account:", self.site_name),
        }
    }
}

#[derive(Template)]
#[template(path = "verify_email.txt")]
struct VerifyEmailText<'a> {
    m: &'a VerifyEmail,
}

#[derive(Template)]
#[template(path = "verify_email.html")]
struct VerifyEmailHtml<'a> {
    m: &'a VerifyEmail,
}

templates!(VerifyEmail, text = VerifyEmailText, html = VerifyEmailHtml);

// ------------------------------------------------------------------ email changed

/// Sent to the *previous* address after a change, so a hijacked account does not go unnoticed.
#[derive(Clone)]
pub struct EmailChanged {
    pub site_name: String,
    pub base_url: String,
    pub username: String,
    /// The new address, partly masked (see [`super::mask_address`]).
    pub new_address: String,
}

impl EmailChanged {
    fn subject(&self) -> String {
        format!("Your {} email address was changed", self.site_name)
    }
}

#[derive(Template)]
#[template(path = "email_changed.txt")]
struct EmailChangedText<'a> {
    m: &'a EmailChanged,
}

#[derive(Template)]
#[template(path = "email_changed.html")]
struct EmailChangedHtml<'a> {
    m: &'a EmailChanged,
}

templates!(EmailChanged, text = EmailChangedText, html = EmailChangedHtml);

impl std::fmt::Debug for PasswordReset {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("PasswordReset").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for VerifyEmail {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("VerifyEmail").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for EmailChanged {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("EmailChanged").finish_non_exhaustive()
    }
}
