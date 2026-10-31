//! Startup defaults for the optional localization provider.

use serde::{Deserialize, Serialize};

/// Deployment defaults; persisted administrator plugin intent takes precedence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct I18nConfig {
    /// Include installed catalogs in locale resolution when the provider starts.
    pub enabled: bool,
    /// Locale used after account, cookie and browser preferences have no match.
    pub default_locale: String,
}

impl Default for I18nConfig {
    fn default() -> Self {
        Self { enabled: false, default_locale: "en".to_owned() }
    }
}

impl I18nConfig {
    /// Performs lightweight configuration validation without linking a catalog engine.
    pub fn validate(&self) -> Result<(), String> {
        if !valid_locale_preference(&self.default_locale) {
            return Err("the i18n default locale must be a language tag of at most 64 ASCII characters".to_owned());
        }
        Ok(())
    }
}

/// Bounded language-tag spelling accepted for configuration and stored preferences.
/// Catalog matching and canonicalization belong to the optional provider.
pub fn valid_locale_preference(locale: &str) -> bool {
    if locale.len() > 64 {
        return false;
    }
    let mut parts = locale.split('-');
    let Some(language) = parts.next() else { return false };
    if !(2..=8).contains(&language.len()) || !language.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return false;
    }
    parts.all(|part| (1..=8).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_alphanumeric()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_english_and_disabled() {
        let config = I18nConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.default_locale, "en");
        config.validate().unwrap();
    }

    #[test]
    fn preference_tags_are_bounded_and_do_not_accept_paths_or_headers() {
        for tag in ["en", "fr-FR", "zh-Hant-TW", "ar", "en-x-private", "qps-ploc"] {
            assert!(valid_locale_preference(tag), "{tag}");
        }
        for tag in ["", "e", "../en", "en_US", "en--US", "en/US", "fr, en;q=1", "en\r\n", "en-abcdefghi"] {
            assert!(!valid_locale_preference(tag), "{tag:?}");
        }
        assert!(!valid_locale_preference(&format!("en-{}", "a-".repeat(32))));
    }
}
