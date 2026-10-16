//! User profile and account settings (display name, bio, email, password).
//!
//! Workstream D owns this file.

use rustit_auth::Passwords;
use rustit_core::{AccountId, Email, User};
use rustit_storage::sqlite::profiles::{AccountProfile, ProfileStore};

use crate::{DomainEvent, ServiceContext, ServiceError};

const MAX_DISPLAY_NAME_LEN: usize = 80;
const MAX_BIO_LEN: usize = 300;
const MAX_WEBSITE_LEN: usize = 200;
const MAX_LOCATION_LEN: usize = 80;
/// Upper bound on password length: Argon2 accepts more, but unbounded input invites denial of service.
const MAX_PASSWORD_LEN: usize = 256;

/// A user's editable profile. `None` means "not set".
pub type Profile = AccountProfile;

/// The profile fields shown on `/{name}` in addition to the account's display name and bio.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublicProfile {
    /// Always an `http://` or `https://` URL.
    pub website: Option<String>,
    pub location: Option<String>,
}

/// Raw "edit profile" input, exactly as submitted. Blank fields clear the value.
#[derive(Debug, Clone, Default)]
pub struct UpdateProfile {
    pub display_name: String,
    pub bio: String,
    pub website: String,
    pub location: String,
}

#[derive(Debug, Clone)]
pub struct ProfileService {
    ctx: ServiceContext,
}

impl ProfileService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }

    fn store(&self) -> ProfileStore {
        self.ctx.database.profiles()
    }

    /// The signed-in user's own profile, for the settings form.
    pub async fn profile(&self, user: &User) -> Result<Profile, ServiceError> {
        self.store().find(user.id()).await?.ok_or(ServiceError::NotFound)
    }

    /// Extras for an account's public profile page. Accounts without any are blank.
    pub async fn public_profile(&self, account_id: AccountId) -> Result<PublicProfile, ServiceError> {
        let profile = self.store().find(account_id).await?.unwrap_or_default();
        Ok(PublicProfile { website: profile.website, location: profile.location })
    }

    /// Validates and replaces every profile field.
    pub async fn update_profile(&self, user: &User, input: UpdateProfile) -> Result<Profile, ServiceError> {
        let profile = Profile {
            display_name: single_line("display name", &input.display_name, MAX_DISPLAY_NAME_LEN)?,
            bio: optional_text("bio", input.bio.trim(), MAX_BIO_LEN)?,
            website: website(&input.website)?,
            location: single_line("location", &input.location, MAX_LOCATION_LEN)?,
        };
        self.store().update(user.id(), &profile).await?;
        tracing::info!(user = %user.name(), "profile updated");
        Ok(profile)
    }

    /// Changes the sign-in email after re-checking the current password.
    ///
    /// [`ServiceError::InvalidCredentials`] for a wrong password, [`ServiceError::Conflict`]
    /// if another account uses the address.
    pub async fn change_email(&self, user: &User, current_password: String, new_email: &str) -> Result<Email, ServiceError> {
        let email: Email = new_email.trim().parse()?;
        self.verify_password(user, current_password).await?;
        self.store().set_email(user.id(), &email).await?;
        tracing::info!(user = %user.name(), "email changed");
        self.ctx.events.publish(DomainEvent::EmailChanged { user: user.id(), previous: user.email.to_string() });
        Ok(email)
    }

    /// Replaces the password after re-checking the current one.
    pub async fn change_password(&self, user: &User, current_password: String, new_password: String) -> Result<(), ServiceError> {
        self.validate_new_password(&new_password)?;
        self.verify_password(user, current_password).await?;
        let hash = Passwords::hash(new_password).await?;
        self.store().set_password_hash(user.id(), &hash).await?;
        tracing::info!(user = %user.name(), "password changed");
        self.ctx.events.publish(DomainEvent::PasswordChanged { user: user.id() });
        Ok(())
    }

    fn validate_new_password(&self, password: &str) -> Result<(), ServiceError> {
        let min = self.ctx.config.security.min_password_length;
        let length = password.chars().count();
        if length < min {
            return Err(ServiceError::Invalid(format!("new password must be at least {min} characters")));
        }
        if length > MAX_PASSWORD_LEN {
            return Err(ServiceError::Invalid(format!("new password must be at most {MAX_PASSWORD_LEN} characters")));
        }
        Ok(())
    }

    /// [`ServiceError::InvalidCredentials`] unless `password` is the user's current password.
    async fn verify_password(&self, user: &User, password: String) -> Result<(), ServiceError> {
        if password.len() > MAX_PASSWORD_LEN * 4 {
            return Err(ServiceError::InvalidCredentials);
        }
        let hash = self.store().password_hash(user.id()).await?.ok_or(ServiceError::NotFound)?;
        if Passwords::verify(password, hash).await? { Ok(()) } else { Err(ServiceError::InvalidCredentials) }
    }
}

/// A trimmed single-line value; blank becomes `None`.
fn single_line(field: &str, value: &str, max: usize) -> Result<Option<String>, ServiceError> {
    let value = value.trim();
    if value.chars().any(char::is_control) {
        return Err(ServiceError::Invalid(format!("{field} must be a single line of text")));
    }
    optional_text(field, value, max)
}

/// `value` (already trimmed) as an optional field of at most `max` characters.
fn optional_text(field: &str, value: &str, max: usize) -> Result<Option<String>, ServiceError> {
    if value.chars().count() > max {
        return Err(ServiceError::Invalid(format!("{field} must be at most {max} characters")));
    }
    if value.chars().any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t') {
        return Err(ServiceError::Invalid(format!("{field} contains invalid characters")));
    }
    Ok((!value.is_empty()).then(|| value.to_owned()))
}

/// An `http(s)://` URL with a host, or `None` when blank. Anything else (notably
/// `javascript:` and `data:` URLs) is rejected, because the value is rendered as a link.
fn website(value: &str) -> Result<Option<String>, ServiceError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > MAX_WEBSITE_LEN {
        return Err(ServiceError::Invalid(format!("website must be at most {MAX_WEBSITE_LEN} characters")));
    }
    let lower = value.to_ascii_lowercase();
    let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://"));
    let has_host = rest.is_some_and(|rest| rest.split(['/', '?', '#']).next().is_some_and(|host| !host.is_empty()));
    if !has_host || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(ServiceError::Invalid("website must be a URL starting with http:// or https://".to_owned()));
    }
    Ok(Some(value.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn website_accepts_http_urls_only() {
        assert_eq!(website("  ").unwrap(), None);
        assert_eq!(website(" https://example.com/me ").unwrap().as_deref(), Some("https://example.com/me"));
        assert_eq!(website("HTTP://Example.com").unwrap().as_deref(), Some("HTTP://Example.com"));
        for bad in ["example.com", "javascript:alert(1)", "https://", "https:///path", "ftp://example.com", "https://exa mple.com"] {
            assert!(website(bad).is_err(), "{bad:?} should be rejected");
        }
        assert!(website(&format!("https://{}", "a".repeat(MAX_WEBSITE_LEN))).is_err());
    }

    #[test]
    fn single_line_fields_reject_line_breaks() {
        assert_eq!(single_line("location", "  Berlin ", 80).unwrap().as_deref(), Some("Berlin"));
        assert_eq!(single_line("location", "", 80).unwrap(), None);
        assert!(single_line("location", "Ber\nlin", 80).is_err());
        assert!(single_line("location", &"é".repeat(81), 80).is_err());
        assert!(single_line("location", &"é".repeat(80), 80).is_ok());
    }

    #[test]
    fn multi_line_text_keeps_line_breaks() {
        assert_eq!(optional_text("bio", "line one\nline two", 300).unwrap().as_deref(), Some("line one\nline two"));
        assert!(optional_text("bio", "bell\u{7}", 300).is_err());
    }
}
