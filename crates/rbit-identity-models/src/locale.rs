//! Canonical persisted account language choices; diagnostic catalogs are not preferences.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum AccountLocale {
    #[serde(rename = "en")]
    English,
    #[serde(rename = "es")]
    Spanish,
    #[serde(rename = "zh-Hans")]
    ChineseSimplified,
    #[serde(rename = "pt-BR")]
    PortugueseBrazil,
    #[serde(rename = "ja")]
    Japanese,
    #[serde(rename = "de")]
    German,
    #[serde(rename = "ru")]
    Russian,
    #[serde(rename = "fr")]
    French,
    #[serde(rename = "ko")]
    Korean,
}

impl AccountLocale {
    pub const ALL: [Self; 9] = [
        Self::English,
        Self::Spanish,
        Self::ChineseSimplified,
        Self::PortugueseBrazil,
        Self::Japanese,
        Self::German,
        Self::Russian,
        Self::French,
        Self::Korean,
    ];

    pub const fn tag(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Spanish => "es",
            Self::ChineseSimplified => "zh-Hans",
            Self::PortugueseBrazil => "pt-BR",
            Self::Japanese => "ja",
            Self::German => "de",
            Self::Russian => "ru",
            Self::French => "fr",
            Self::Korean => "ko",
        }
    }

    /// Settings accept only exact canonical tags. Header lookup is a separate concern.
    pub fn parse(tag: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|locale| locale.tag() == tag)
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
        for invalid in [
            "", "EN", "es-MX", " es", "en-XA", "ar-XB", "../es", "<script>", "en\nes", "zh", "zh-hans", "zh-Hant", "zh-CN", "pt", "pt-PT",
            "DE",
        ] {
            assert_eq!(AccountLocale::parse(invalid), None);
            assert!(serde_json::from_str::<AccountLocale>(&format!("{invalid:?}")).is_err());
        }
    }
}
