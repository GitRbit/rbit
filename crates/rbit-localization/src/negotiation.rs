//! Bounded basic-language-range negotiation with allowlist lookup fallback.

use crate::Locale;

const MAX_HEADER_BYTES: usize = 4096;
const MAX_RANGES: usize = 32;
const MAX_TAG_BYTES: usize = 63;
const LOCALES: [Locale; 3] = [Locale::English, Locale::PseudoExpanded, Locale::PseudoRtl];

/// Borrowed choices in descending priority; strings must contain a single tag.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalePreferences<'a> {
    /// A validated locale-setting request, never an arbitrary redirect destination.
    pub explicit: Option<&'a str>,
    /// The authenticated account's saved preference.
    pub account: Option<&'a str>,
    /// The anonymous preference cookie's value.
    pub anonymous: Option<&'a str>,
    /// Combined `Accept-Language` header, if it is valid UTF-8.
    pub accept_language: Option<&'a str>,
}

/// Why a locale was selected, for cache and preference adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreferenceSource {
    Explicit,
    Account,
    Anonymous,
    AcceptLanguage,
    Default,
}

/// A supported locale together with its selection provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedLocale {
    pub locale: Locale,
    pub source: PreferenceSource,
}

/// A pseudo locale cannot be the default while pseudo catalogs are disabled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnsupportedDefault;

/// Immutable configuration. Negotiation allocates nothing and examines bounded input.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocaleResolver {
    default: Locale,
    enable_pseudo: bool,
}

impl LocaleResolver {
    /// Enables pseudo catalogs explicitly; production defaults supply only English.
    pub fn new(default: Locale, enable_pseudo: bool) -> Result<Self, UnsupportedDefault> {
        if default != Locale::English && !enable_pseudo {
            return Err(UnsupportedDefault);
        }
        Ok(Self { default, enable_pseudo })
    }

    /// Invalid/unsupported choices fall through. If nothing is acceptable, serve the
    /// instance default (HTTP's permitted disregard behavior), rather than return 406.
    pub fn resolve(self, preferences: LocalePreferences<'_>) -> ResolvedLocale {
        for (tag, source) in [
            (preferences.explicit, PreferenceSource::Explicit),
            (preferences.account, PreferenceSource::Account),
            (preferences.anonymous, PreferenceSource::Anonymous),
        ] {
            if let Some(locale) = tag.and_then(|tag| self.lookup(tag)) {
                return ResolvedLocale { locale, source };
            }
        }
        if let Some(locale) = preferences.accept_language.and_then(|header| self.negotiate(header)) {
            return ResolvedLocale { locale, source: PreferenceSource::AcceptLanguage };
        }
        ResolvedLocale { locale: self.default, source: PreferenceSource::Default }
    }

    fn supported(self, locale: Locale) -> bool {
        locale == Locale::English || self.enable_pseudo
    }

    fn lookup(self, tag: &str) -> Option<Locale> {
        if !valid_range(tag) || tag == "*" {
            return None;
        }
        let mut range = tag;
        loop {
            if let Some(locale) = LOCALES.iter().copied().find(|locale| range.eq_ignore_ascii_case(locale.tag())) {
                // Disabled pseudo choices must not silently become English.
                return self.supported(locale).then_some(locale);
            }
            let (parent, _) = range.rsplit_once('-')?;
            range = parent;
            // RFC 4647 lookup removes a trailing singleton with its extension.
            if range.rsplit('-').next().is_some_and(|part| part.len() == 1) {
                range = range.rsplit_once('-')?.0;
            }
        }
    }

    fn negotiate(self, header: &str) -> Option<Locale> {
        if header.len() > MAX_HEADER_BYTES || !header.is_ascii() || header.bytes().any(|b| b.is_ascii_control() && b != b'\t') {
            return None;
        }
        let mut ranges = [None; MAX_RANGES];
        for (index, member) in header.split(',').enumerate() {
            if index == MAX_RANGES {
                return None;
            }
            ranges[index] = parse_member(member);
        }
        let mut winner = None;
        for locale in LOCALES.into_iter().filter(|locale| self.supported(*locale)) {
            let mut governing = None;
            for (index, &(range, quality)) in ranges.iter().flatten().enumerate() {
                let specificity = if range == "*" {
                    // A wildcard should never opt users into a diagnostic catalog.
                    if locale != self.default {
                        continue;
                    }
                    0
                } else if self.lookup(range) == Some(locale) {
                    range.split('-').count()
                } else {
                    continue;
                };
                if governing.is_none_or(|(old, _, _)| specificity > old) {
                    governing = Some((specificity, quality, index));
                }
            }
            if let Some((_, quality, index)) = governing
                && quality > 0
                && winner.is_none_or(|(_, best, order)| quality > best || (quality == best && index < order))
            {
                winner = Some((locale, quality, index));
            }
        }
        winner.map(|(locale, _, _)| locale)
    }
}

fn valid_range(range: &str) -> bool {
    if range == "*" {
        return true;
    }
    if range.is_empty() || range.len() > MAX_TAG_BYTES {
        return false;
    }
    range.split('-').enumerate().all(|(index, part)| {
        !part.is_empty()
            && part.len() <= 8
            && part.bytes().all(|byte| if index == 0 { byte.is_ascii_alphabetic() } else { byte.is_ascii_alphanumeric() })
    })
}

fn parse_member(member: &str) -> Option<(&str, u16)> {
    let mut parts = member.split(';');
    let range = parts.next()?.trim();
    if !valid_range(range) {
        return None;
    }
    let quality = match parts.next() {
        None => 1000,
        Some(parameter) => {
            let (name, value) = parameter.trim().split_once('=')?;
            if !name.eq_ignore_ascii_case("q") {
                return None;
            }
            parse_quality(value)?
        }
    };
    if parts.next().is_some() {
        return None;
    }
    Some((range, quality))
}

fn parse_quality(value: &str) -> Option<u16> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if !matches!(whole, "0" | "1") || fraction.len() > 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let fraction = fraction.bytes().fold(0_u16, |result, byte| result * 10 + u16::from(byte - b'0'))
        * 10_u16.pow(u32::try_from(3 - fraction.len()).ok()?);
    if whole == "1" { (fraction == 0).then_some(1000) } else { Some(fraction) }
}
