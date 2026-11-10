//! Times, dates, counts and sizes in the page's language.
//!
//! A view keeps the moment itself ([`When`]) and the template formats it with the page's
//! locale: `{% call ui::localized(row.when.relative(page.locale)) %}{% endcall %}`, beside
//! `datetime="{{ row.when.iso() }}"`, which stays ISO 8601 in every language. Counts and
//! sizes go through [`count`] and [`bytes`]; ids, URLs and attributes keep plain digits.
//! The wording lives in the catalog (`rbit_localization::time`), the separators in
//! `rbit_localization::number`.
use std::{borrow::Borrow, time::Duration};

use rbit_core::Timestamp;
use rbit_localization::{
    Catalog, Locale, Localized, number,
    time::{CivilDate, TimeMessage},
};
use time::OffsetDateTime;

/// A moment shown to people. Copy it into a view instead of a preformatted string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct When(pub Timestamp);

impl From<Timestamp> for When {
    fn from(at: Timestamp) -> Self {
        Self(at)
    }
}

impl When {
    /// Machine-readable ISO 8601 for `<time datetime>`, the same in every language.
    pub fn iso(self) -> String {
        self.0.iso8601()
    }

    /// `3 hours ago`, `vor 3 Stunden`, `3 時間前`; `2 years ago` from a year on.
    pub fn relative(self, locale: impl Borrow<Locale>) -> Localized {
        relative(self.0, *locale.borrow())
    }

    /// `in 3 months` for a moment ahead, such as an expiry.
    pub fn until(self, locale: impl Borrow<Locale>) -> Localized {
        until(self.0, *locale.borrow())
    }

    /// `Nov 8, 2026`, `8. Nov. 2026`, `2026年11月8日`.
    pub fn date(self, locale: impl Borrow<Locale>) -> Localized {
        date(self.0, *locale.borrow())
    }
}

fn civil(at: Timestamp) -> CivilDate {
    let date = OffsetDateTime::from_unix_timestamp(at.seconds()).unwrap_or(OffsetDateTime::UNIX_EPOCH).date();
    CivilDate { year: date.year(), month: date.month().into(), day: date.day() }
}

/// [`When::relative`] measured from `now`.
pub fn relative_at(at: Timestamp, now: Timestamp, locale: Locale) -> Localized {
    let ago = u64::try_from(now.seconds() - at.seconds()).unwrap_or_default();
    Catalog::new(locale).localized(TimeMessage::past(ago))
}

/// `at` relative to now, in `locale`.
pub fn relative(at: Timestamp, locale: Locale) -> Localized {
    relative_at(at, Timestamp::now(), locale)
}

/// [`When::until`] measured from `now`.
pub fn until_at(at: Timestamp, now: Timestamp, locale: Locale) -> Localized {
    let ahead = u64::try_from(at.seconds() - now.seconds()).unwrap_or_default();
    Catalog::new(locale).localized(TimeMessage::future(ahead).unwrap_or_else(|| TimeMessage::OnDate(civil(at))))
}

/// A moment ahead of now, in `locale`.
pub fn until(at: Timestamp, locale: Locale) -> Localized {
    until_at(at, Timestamp::now(), locale)
}

/// The calendar date of `at` (UTC), in `locale`.
pub fn date(at: Timestamp, locale: Locale) -> Localized {
    Catalog::new(locale).localized(TimeMessage::Date(civil(at)))
}

/// A count: `1,234` (`1.234` in German).
pub fn integer(locale: Locale, value: u64) -> String {
    number::integer(locale, value)
}

/// `1234567` → `1,234,567` (`1.234.567` in German, `1 234 567` in French).
pub fn count(locale: Locale, value: i64) -> String {
    number::signed(locale, value)
}

/// A measurement with one decimal: `12.5` (`12,5` in German).
pub fn decimal(locale: Locale, value: f64) -> String {
    let text = format!("{value:.1}");
    let mark = number::decimal_separator(locale);
    if mark == '.' { text } else { text.replace('.', &mark.to_string()) }
}

/// Binary units with one decimal: `0 B`, `512 B`, `1.5 KiB` (`1,5 KiB` in German).
pub fn bytes(locale: Locale, value: u64) -> String {
    const UNITS: [&str; 5] = ["KiB", "MiB", "GiB", "TiB", "PiB"];
    if value < 1024 {
        return format!("{} B", number::integer(locale, value));
    }
    let mut unit = 0;
    let mut scaled = value;
    while scaled >= 1024 * 1024 && unit < UNITS.len() - 1 {
        scaled /= 1024;
        unit += 1;
    }
    // `scaled` is now in [1024, 1024²): divide once more with one decimal of precision.
    format!("{} {}", number::tenths(locale, scaled * 10 / 1024), UNITS[unit])
}

/// The two most significant units of a running time: `3 hours 5 minutes`, `2 days`.
pub fn uptime(locale: Locale, duration: Duration) -> Localized {
    Catalog::new(locale).localized(TimeMessage::duration(duration.as_secs()))
}

/// A file's size; templates show it with [`Size::text`] (`1.4 KB`, `1,4 KB`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Size(pub u64);

impl Size {
    pub fn text(self, locale: impl Borrow<Locale>) -> String {
        crate::render::format_size(self.0, *locale.borrow())
    }
}

/// How long an Actions job or step took, in seconds; templates show it with [`Took::text`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Took(pub i64);

impl Took {
    /// `1h 2m`, `1 Std. 2 Min.`, `1時間2分`.
    pub fn text(self, locale: impl Borrow<Locale>) -> Localized {
        elapsed(*locale.borrow(), self.0)
    }
}

/// How long something took, in narrow units: `1h 2m`, `1m 3s`, `12s`.
pub fn elapsed(locale: Locale, seconds: i64) -> Localized {
    Catalog::new(locale).localized(TimeMessage::Elapsed { seconds: u64::try_from(seconds).unwrap_or_default() })
}

#[cfg(test)]
mod tests;
