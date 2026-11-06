//! Canonical persisted account language choices; diagnostic catalogs are not preferences.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum AccountLocale {
    #[serde(rename = "en")]
    English,
    #[serde(rename = "es")]
    Spanish,
}

impl AccountLocale {
    pub const ALL: [Self; 2] = [Self::English, Self::Spanish];

    pub const fn tag(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Spanish => "es",
        }
    }

    /// Settings accept only exact canonical tags. Header lookup is a separate concern.
    pub fn parse(tag: &str) -> Option<Self> {
        match tag {
            "en" => Some(Self::English),
            "es" => Some(Self::Spanish),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_choices_are_closed_canonical_and_never_diagnostic() {
        for locale in AccountLocale::ALL {
            assert_eq!(AccountLocale::parse(locale.tag()), Some(locale));
            assert_eq!(serde_json::to_string(&locale).unwrap(), format!("\"{}\"", locale.tag()));
        }
        for invalid in ["", "EN", "es-MX", " es", "en-XA", "ar-XB", "../es", "<script>", "en\nes"] {
            assert_eq!(AccountLocale::parse(invalid), None);
            assert!(serde_json::from_str::<AccountLocale>(&format!("{invalid:?}")).is_err());
        }
    }
}
