//! Interface preferences that follow a user across devices.
//!
//! Frontend workstream T owns this file. The service only checks that a theme slug is
//! well-formed; which themes exist is a presentation concern decided by the web layer's
//! catalog, which also falls back to the default for slugs it no longer knows.

use rbit_core::User;
use rbit_storage::sqlite::preferences::PreferenceStore;

use crate::{ServiceContext, ServiceError};

/// Longest accepted theme slug (the catalog's longest is far shorter).
pub const MAX_THEME_LEN: usize = 64;

#[derive(Debug, Clone)]
pub struct PreferenceService {
    ctx: ServiceContext,
}

impl PreferenceService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }

    fn store(&self) -> PreferenceStore {
        self.ctx.database.preferences()
    }

    /// The user's stored theme slug, if they ever chose one.
    pub async fn theme(&self, user: &User) -> Result<Option<String>, ServiceError> {
        Ok(self.store().theme(user.id()).await?)
    }

    /// Remembers `theme` for `user`. Slugs are `[a-z0-9-]`, 1–64 characters.
    pub async fn set_theme(&self, user: &User, theme: &str) -> Result<(), ServiceError> {
        if !is_theme_slug(theme) {
            return Err(ServiceError::Invalid("choose one of the listed themes".to_owned()));
        }
        Ok(self.store().set_theme(user.id(), theme).await?)
    }
}

/// Whether `value` has the shape of a theme slug.
pub fn is_theme_slug(value: &str) -> bool {
    (1..=MAX_THEME_LEN).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_slugs_are_lowercase_words_and_dashes() {
        assert!(is_theme_slug("dark"));
        assert!(is_theme_slug("base16-gruvbox-dark-hard"));
        assert!(!is_theme_slug(""));
        assert!(!is_theme_slug("Dark"));
        assert!(!is_theme_slug("../etc"));
        assert!(!is_theme_slug("a b"));
        assert!(!is_theme_slug(&"a".repeat(MAX_THEME_LEN + 1)));
    }
}
