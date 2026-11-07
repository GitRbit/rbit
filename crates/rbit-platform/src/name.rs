use crate::PlatformError;
use std::fmt;

/// Generated native name only: titles and user paths never belong in this type.
#[derive(Clone, PartialEq, Eq)]
pub struct Component(String);
impl Component {
    pub fn new(value: &str) -> Result<Self, PlatformError> {
        if value.is_empty()
            || value.len() > 64
            || value.ends_with('.')
            || value == "."
            || value == ".."
            || !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(PlatformError::InvalidName);
        }
        let stem = value.split('.').next().unwrap_or_default().to_ascii_uppercase();
        let numbered = stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|number| number.len() == 1 && number.as_bytes()[0].is_ascii_digit());
        if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered {
            return Err(PlatformError::InvalidName);
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for Component {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Component(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_names_exclude_aliases_traversal_ads_and_user_text() {
        for value in [
            "", ".", "..", "../x", "a/b", "a\\b", "C:", "x:stream", "NUL.txt", "cOn", "LPT9", "COM1.bin", "ends.", "ends ", "a\0b", "é",
            "a%2fb",
        ] {
            assert!(matches!(Component::new(value), Err(PlatformError::InvalidName)), "{value:?}");
        }
        assert!(Component::new(&"x".repeat(65)).is_err());
        for value in ["objects", "ab", "123.git", "wiki-store-v1", "HEAD", ".stage-0123456789abcdef"] {
            assert_eq!(Component::new(value).unwrap().as_str(), value);
        }
    }
}
