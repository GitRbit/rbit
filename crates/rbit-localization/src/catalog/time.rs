//! Times and dates as people read them: relative times ("2 hours ago", "in 3 days"),
//! calendar dates ("Nov 8, 2026") and durations ("3 hours 5 minutes", "1m 3s").
//!
//! [`TimeMessage`] is a table-backed family like the others: each locale writes its phrases
//! (with its plural categories) and its date pattern in `translations/<locale>/time.rs`.
//! This module only decides *which* phrase a span of seconds gets, the same for every
//! language, so a page reads "3 hours ago" in English exactly where German reads
//! "vor 3 Stunden". Machine values (`datetime` attributes, API fields) never come from here.
use super::fallback::{MessageFamily, MessageWriter};
use crate::locale::Locale;

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;
const MONTH: u64 = 30 * DAY;
const YEAR: u64 = 365 * DAY;

/// A unit of a relative time or a duration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TimeUnit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

impl TimeUnit {
    /// Every unit, smallest first, for checks and tooling.
    pub const ALL: [Self; 7] = [Self::Second, Self::Minute, Self::Hour, Self::Day, Self::Week, Self::Month, Self::Year];
}

/// A whole number of one unit: "3 hours".
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Amount {
    pub unit: TimeUnit,
    pub count: u64,
}

impl Amount {
    pub const fn new(count: u64, unit: TimeUnit) -> Self {
        Self { unit, count }
    }
}

/// A day of the proleptic Gregorian calendar (UTC), as the caller's clock library gives it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CivilDate {
    pub year: i32,
    /// 1 (January) to 12.
    pub month: u8,
    /// 1 to 31.
    pub day: u8,
}

/// Times, dates and durations. Each variant documents its English text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeMessage {
    /// "just now": less than a minute ago.
    JustNow,
    /// "3 hours ago".
    Ago(Amount),
    /// "in 3 hours".
    In(Amount),
    /// "in under a minute".
    WithinAMinute,
    /// "on Nov 8, 2026": a relative time too far away to count, read in a sentence.
    OnDate(CivilDate),
    /// "Nov 8, 2026".
    Date(CivilDate),
    /// "3 hours 5 minutes", or "45 seconds" without the smaller part: a running time such
    /// as the server's uptime.
    Duration { major: Amount, minor: Option<Amount> },
    /// "1h 2m", "1m 3s" or "12s": how long a job or step took, in narrow units.
    Elapsed { seconds: u64 },
}

impl TimeMessage {
    /// The phrase for a moment `seconds` in the past: [`JustNow`](Self::JustNow), or
    /// [`Ago`](Self::Ago) in minutes, hours, days or months. `None` from a year on, where a
    /// page shows [`OnDate`](Self::OnDate) instead.
    pub const fn past(seconds: u64) -> Option<Self> {
        let (count, unit) = match seconds {
            s if s < MINUTE => return Some(Self::JustNow),
            s if s < HOUR => (s / MINUTE, TimeUnit::Minute),
            s if s < DAY => (s / HOUR, TimeUnit::Hour),
            s if s < MONTH => (s / DAY, TimeUnit::Day),
            s if s < YEAR => (s / MONTH, TimeUnit::Month),
            _ => return None,
        };
        Some(Self::Ago(Amount::new(count, unit)))
    }

    /// The phrase for a moment `seconds` ahead (an expiry, the next sync), rounded rather
    /// than floored: a 90-day expiry seen an hour later is still "in 3 months". `None` from
    /// a year on, where a page shows [`OnDate`](Self::OnDate) instead.
    pub fn future(seconds: u64) -> Option<Self> {
        let round = |unit: u64| (seconds + unit / 2) / unit;
        let (count, unit) = if seconds < MINUTE {
            return Some(Self::WithinAMinute);
        } else if round(MINUTE) < 60 {
            (round(MINUTE), TimeUnit::Minute)
        } else if round(HOUR) < 24 {
            (round(HOUR), TimeUnit::Hour)
        } else if round(DAY) < 30 {
            (round(DAY), TimeUnit::Day)
        } else if seconds < YEAR {
            (round(MONTH), TimeUnit::Month)
        } else {
            return None;
        };
        Some(Self::In(Amount::new(count, unit)))
    }

    /// The two most significant units of a running time: `45 seconds`, `3 hours 5 minutes`,
    /// `2 days` (a zero smaller part is left out).
    pub const fn duration(seconds: u64) -> Self {
        let (major, minor) = match seconds {
            s if s < MINUTE => return Self::Duration { major: Amount::new(s, TimeUnit::Second), minor: None },
            s if s < HOUR => (Amount::new(s / MINUTE, TimeUnit::Minute), Amount::new(s % MINUTE, TimeUnit::Second)),
            s if s < DAY => (Amount::new(s / HOUR, TimeUnit::Hour), Amount::new(s % HOUR / MINUTE, TimeUnit::Minute)),
            s => (Amount::new(s / DAY, TimeUnit::Day), Amount::new(s % DAY / HOUR, TimeUnit::Hour)),
        };
        Self::Duration { major, minor: if minor.count == 0 { None } else { Some(minor) } }
    }
}

/// The parts of an [`TimeMessage::Elapsed`] time: hours and minutes from an hour on,
/// minutes and seconds from a minute on, otherwise seconds.
pub(crate) const fn elapsed_parts(seconds: u64) -> (u64, TimeUnit, Option<(u64, TimeUnit)>) {
    let (hours, minutes, rest) = (seconds / HOUR, seconds % HOUR / MINUTE, seconds % MINUTE);
    match (hours, minutes) {
        (0, 0) => (rest, TimeUnit::Second, None),
        (0, _) => (minutes, TimeUnit::Minute, Some((rest, TimeUnit::Second))),
        _ => (hours, TimeUnit::Hour, Some((minutes, TimeUnit::Minute))),
    }
}

/// Writes an elapsed time with a locale's narrow `unit` names: each number, `gap`, its
/// unit, and `between` the two parts.
pub(crate) fn write_elapsed(text: &mut MessageWriter, seconds: u64, unit: fn(TimeUnit) -> &'static str, gap: &str, between: &str) {
    let (count, first, second) = elapsed_parts(seconds);
    text.number(count);
    text.literal(gap);
    text.literal(unit(first));
    if let Some((count, second)) = second {
        text.literal(between);
        text.number(count);
        text.literal(gap);
        text.literal(unit(second));
    }
}

/// Writes a duration's parts with `part` (one amount in the locale's words) and `between`.
pub(crate) fn write_duration(
    text: &mut MessageWriter,
    major: Amount,
    minor: Option<Amount>,
    part: fn(&mut MessageWriter, Amount),
    between: &str,
) {
    part(text, major);
    if let Some(minor) = minor {
        text.literal(between);
        part(text, minor);
    }
}

const ENGLISH_MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// The entry of `months` for `date`'s month (clamped to December).
pub(crate) fn month(months: &[&'static str; 12], date: CivilDate) -> &'static str {
    months[usize::from(date.month.clamp(1, 12)) - 1]
}

fn english_amount(text: &mut MessageWriter, amount: Amount) {
    text.number(amount.count);
    let unit = match amount.unit {
        TimeUnit::Second => " second",
        TimeUnit::Minute => " minute",
        TimeUnit::Hour => " hour",
        TimeUnit::Day => " day",
        TimeUnit::Week => " week",
        TimeUnit::Month => " month",
        TimeUnit::Year => " year",
    };
    text.literal(unit);
    if amount.count != 1 {
        text.literal("s");
    }
}

fn english_date(text: &mut MessageWriter, date: CivilDate) {
    text.literal(month(&ENGLISH_MONTHS, date));
    text.literal(" ");
    text.digits(date.day.into());
    text.literal(", ");
    text.digits(date.year.into());
}

impl MessageFamily for TimeMessage {
    fn id(self) -> &'static str {
        match self {
            Self::JustNow => "time.just-now",
            Self::Ago(..) => "time.ago",
            Self::In(..) => "time.in",
            Self::WithinAMinute => "time.within-a-minute",
            Self::OnDate(..) => "time.on-date",
            Self::Date(..) => "time.date",
            Self::Duration { .. } => "time.duration",
            Self::Elapsed { .. } => "time.elapsed",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::JustNow => text.literal("just now"),
            Self::Ago(amount) => {
                english_amount(text, amount);
                text.literal(" ago");
            }
            Self::In(amount) => {
                text.literal("in ");
                english_amount(text, amount);
            }
            Self::WithinAMinute => text.literal("in under a minute"),
            Self::OnDate(date) => {
                text.literal("on ");
                english_date(text, date);
            }
            Self::Date(date) => english_date(text, date),
            Self::Duration { major, minor } => write_duration(text, major, minor, english_amount, " "),
            Self::Elapsed { seconds } => write_elapsed(
                text,
                seconds,
                |unit| match unit {
                    TimeUnit::Hour => "h",
                    TimeUnit::Minute => "m",
                    _ => "s",
                },
                "",
                " ",
            ),
        }
    }

    fn translate(self, locale: Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
