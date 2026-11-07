//! Logical page route names; disk storage must use stable IDs, never these strings.
use std::{fmt, str::FromStr};

use unicode_normalization::{UnicodeNormalization, char::is_combining_mark, is_nfc};

use super::{WikiNameError, unsupported_display_control};

/// NFC, case-sensitive slash-separated logical path. Accepts ALREADY-DECODED input
/// only; residual percent is rejected rather than decoded a second time. The HTTP
/// adapter must reject encoded separators and decode exactly once before construction.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WikiPath(String);

impl WikiPath {
    pub const MAX_RAW_BYTES: usize = 1024;
    pub const MAX_BYTES: usize = 512;
    pub const MAX_COMPONENT_BYTES: usize = 64;
    pub const MAX_COMPONENTS: usize = 8;

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for WikiPath {
    type Error = WikiNameError;

    fn try_from(mut value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(WikiNameError::Empty);
        }
        if value.len() > Self::MAX_RAW_BYTES {
            return Err(WikiNameError::RawLimit);
        }
        if !is_nfc(&value) {
            value = value.nfc().collect();
        }
        if value.len() > Self::MAX_BYTES {
            return Err(WikiNameError::TooLong);
        }
        for (index, component) in value.split('/').enumerate() {
            if index >= Self::MAX_COMPONENTS {
                return Err(WikiNameError::TooDeep);
            }
            validate_component(component)?;
        }
        Ok(Self(value))
    }
}

fn validate_component(component: &str) -> Result<(), WikiNameError> {
    if component.len() > WikiPath::MAX_COMPONENT_BYTES {
        return Err(WikiNameError::TooLong);
    }
    if !component.chars().next().is_some_and(char::is_alphanumeric) || component.ends_with('.') || component.ends_with(' ') {
        return Err(WikiNameError::InvalidBoundary);
    }
    if component.chars().any(|character| {
        unsupported_display_control(character)
            || matches!(character, '\u{034f}' | '\u{180b}'..='\u{180f}' | '\u{fe00}'..='\u{fe0f}' | '\u{e0100}'..='\u{e01ef}')
            || !(character.is_alphanumeric() || is_combining_mark(character) || matches!(character, '-' | '_' | '.'))
    }) {
        return Err(WikiNameError::InvalidCharacter);
    }
    let stem = component.split('.').next().unwrap_or_default();
    let device = ["con", "prn", "aux", "nul"].iter().any(|reserved| stem.eq_ignore_ascii_case(reserved))
        || (stem.get(..3).is_some_and(|prefix| prefix.eq_ignore_ascii_case("com") || prefix.eq_ignore_ascii_case("lpt"))
            && matches!(&stem[3..], "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "\u{00b9}" | "\u{00b2}" | "\u{00b3}"));
    let suffix = component.rsplit_once('.').map(|(_, suffix)| suffix);
    if device
        || component.contains("..")
        || suffix.is_some_and(|suffix| suffix.eq_ignore_ascii_case("git") || suffix.eq_ignore_ascii_case("lock"))
    {
        return Err(WikiNameError::ReservedPath);
    }
    Ok(())
}

impl FromStr for WikiPath {
    type Err = WikiNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() > Self::MAX_RAW_BYTES {
            return Err(WikiNameError::RawLimit);
        }
        Self::try_from(value.to_owned())
    }
}

impl fmt::Display for WikiPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
