use intl_pluralrules::{PluralRuleType, PluralRules};
use unic_langid::LanguageIdentifier;

use crate::PackError;

/// Canonical, supported language identifiers only: extensions/private-use subtags
/// are outside pack schema 1, and unknown plural-rule languages are rejected.
pub(crate) fn validate_locale(value: &str, expected_direction: Option<&str>) -> Result<LanguageIdentifier, PackError> {
    if value.len() > 64 {
        return Err(PackError::Locale);
    }
    let locale: LanguageIdentifier = value.parse().map_err(|_| PackError::Locale)?;
    let canonical = locale.to_string();
    if canonical != value {
        return Err(PackError::Locale);
    }
    let base: LanguageIdentifier = locale.language.to_string().parse().map_err(|_| PackError::Locale)?;
    if PluralRules::create(locale.clone(), PluralRuleType::CARDINAL).is_err()
        && PluralRules::create(base, PluralRuleType::CARDINAL).is_err()
    {
        return Err(PackError::Locale);
    }
    if expected_direction.is_some_and(|direction| direction != direction_for(&locale)) {
        return Err(PackError::Locale);
    }
    Ok(locale)
}

pub(crate) fn direction_for(locale: &LanguageIdentifier) -> &'static str {
    if let Some(script) = locale.script {
        return if matches!(
            script.as_str(),
            "Adlm"
                | "Arab"
                | "Armi"
                | "Avst"
                | "Chrs"
                | "Cprt"
                | "Elym"
                | "Gara"
                | "Hatr"
                | "Hebr"
                | "Hung"
                | "Khar"
                | "Lydi"
                | "Mand"
                | "Mani"
                | "Mend"
                | "Merc"
                | "Mero"
                | "Narb"
                | "Nbat"
                | "Nkoo"
                | "Orkh"
                | "Ougr"
                | "Palm"
                | "Phli"
                | "Phlp"
                | "Phnx"
                | "Prti"
                | "Rohg"
                | "Samr"
                | "Sarb"
                | "Sogd"
                | "Sogo"
                | "Syrc"
                | "Thaa"
                | "Yezi"
        ) {
            "rtl"
        } else {
            "ltr"
        };
    }
    if matches!(locale.language.as_str(), "ar" | "he" | "fa" | "ur" | "ps" | "yi" | "dv" | "syr" | "ug" | "sd" | "ckb" | "nqo") {
        "rtl"
    } else {
        "ltr"
    }
}

/// Bounded browser preference parsing, preserving explicit exclusions. Only the
/// RFC q-value decimal grammar is accepted (0/1 with up to three fraction digits).
#[derive(Debug)]
pub(crate) struct Preference {
    pub range: String,
    pub quality: u16,
}
pub(crate) fn browser_preferences(header: &str) -> Vec<Preference> {
    if header.len() > 1024 {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    for entry in header.split(',').take(16) {
        let mut fields = entry.trim().split(';');
        let Some(range) = fields.next() else { continue };
        let range = if range == "*" {
            "*".to_owned()
        } else {
            let Ok(locale) = range.parse::<LanguageIdentifier>() else { continue };
            locale.to_string()
        };
        let quality = match fields.next() {
            None => 1000,
            Some(field) => {
                let Some(quality) = field.trim().strip_prefix("q=").and_then(quality) else { continue };
                quality
            }
        };
        if fields.next().is_some() {
            continue;
        }
        candidates.push(Preference { range, quality });
    }
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.quality));
    candidates
}
fn quality(value: &str) -> Option<u16> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if !matches!(whole, "0" | "1") || fraction.len() > 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if whole == "1" {
        return fraction.bytes().all(|byte| byte == b'0').then_some(1000);
    }
    let value = if fraction.is_empty() { 0 } else { fraction.parse::<u16>().ok()? };
    Some(
        value
            * match fraction.len() {
                0 | 3 => 1,
                1 => 100,
                _ => 10,
            },
    )
}

/// A more specific language range overrides wildcard or broad exclusions.
pub(crate) fn browser_allows(locale: &str, preferences: &[Preference]) -> bool {
    preferences
        .iter()
        .filter(|preference| {
            preference.range == "*"
                || locale == preference.range
                || locale.strip_prefix(&preference.range).is_some_and(|suffix| suffix.starts_with('-'))
        })
        .max_by_key(|preference| preference.range.len())
        .is_none_or(|preference| preference.quality > 0)
}
