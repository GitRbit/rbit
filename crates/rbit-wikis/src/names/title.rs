//! User-facing Unicode title, independent from routing and storage names.
use std::{fmt, str::FromStr};

use unicode_normalization::{UnicodeNormalization, is_nfc};

use super::{WikiNameError, unsupported_display_control};

/// Trimmed NFC title. Punctuation and emoji remain text, not trusted HTML.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WikiTitle(String);

impl WikiTitle {
    pub const MAX_RAW_BYTES: usize = 1024;
    pub const MAX_CHARACTERS: usize = 200;

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Decode catalog bytes without trimming or normalizing persisted metadata.
    pub fn from_canonical(value: String) -> Result<Self, WikiNameError> {
        if value.len() > Self::MAX_RAW_BYTES {
            return Err(WikiNameError::RawLimit);
        }
        if !is_nfc(&value) || value.trim().len() != value.len() {
            return Err(WikiNameError::NonCanonical);
        }
        Self::try_from(value)
    }
}

impl TryFrom<String> for WikiTitle {
    type Error = WikiNameError;

    fn try_from(mut value: String) -> Result<Self, Self::Error> {
        if value.len() > Self::MAX_RAW_BYTES {
            return Err(WikiNameError::RawLimit);
        }
        if value.chars().any(unsupported_display_control) {
            return Err(WikiNameError::InvalidCharacter);
        }
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(WikiNameError::Empty);
        }
        if trimmed.len() != value.len() {
            value = trimmed.to_owned();
        }
        if !is_nfc(&value) {
            value = value.nfc().collect();
        }
        if value.chars().take(Self::MAX_CHARACTERS + 1).count() > Self::MAX_CHARACTERS {
            return Err(WikiNameError::TooLong);
        }
        Ok(Self(value))
    }
}

impl FromStr for WikiTitle {
    type Err = WikiNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() > Self::MAX_RAW_BYTES {
            return Err(WikiNameError::RawLimit);
        }
        Self::try_from(value.to_owned())
    }
}

impl fmt::Display for WikiTitle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
