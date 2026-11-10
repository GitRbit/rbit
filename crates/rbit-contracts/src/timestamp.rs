//! A Unix timestamp (seconds) with human-friendly formatting helpers for templates.

use std::fmt;

use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339, macros::format_description};

const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
const MONTH: i64 = 30 * DAY;
const YEAR: i64 = 365 * DAY;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Timestamp(pub i64);

impl Timestamp {
    pub fn now() -> Self {
        Self(OffsetDateTime::now_utc().unix_timestamp())
    }

    pub const fn seconds(self) -> i64 {
        self.0
    }

    fn to_datetime(self) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(self.0).unwrap_or(OffsetDateTime::UNIX_EPOCH)
    }

    /// Machine-readable form for `<time datetime="…">`.
    pub fn iso8601(self) -> String {
        self.to_datetime().format(&Rfc3339).unwrap_or_default()
    }

    /// e.g. `Oct 15, 2026`.
    pub fn date(self) -> String {
        self.to_datetime().format(format_description!("[month repr:short] [day padding:none], [year]")).unwrap_or_default()
    }

    /// e.g. `3 hours ago`, `5 months ago`, `2 years ago`.
    pub fn relative(self) -> String {
        Self::relative_between(self, Self::now())
    }

    fn relative_between(then: Self, now: Self) -> String {
        let delta = (now.0 - then.0).max(0);
        let (amount, unit) = match delta {
            d if d < MINUTE => return "just now".to_owned(),
            d if d < HOUR => (d / MINUTE, "minute"),
            d if d < DAY => (d / HOUR, "hour"),
            d if d < MONTH => (d / DAY, "day"),
            d if d < YEAR => (d / MONTH, "month"),
            d => (d / YEAR, "year"),
        };
        let plural = if amount == 1 { "" } else { "s" };
        format!("{amount} {unit}{plural} ago")
    }

    /// For a time ahead, such as an expiry: e.g. `in 3 months`, falling back to an absolute
    /// date after a year. [`relative`](Self::relative) would say "just now" for any of them.
    pub fn until(self) -> String {
        Self::until_between(self, Self::now())
    }

    fn until_between(then: Self, now: Self) -> String {
        let delta = (then.0 - now.0).max(0);
        // Rounded, not floored: a 90-day expiry seen an hour later is still "in 3 months".
        let round = |unit: i64| (delta + unit / 2) / unit;
        let (amount, unit) = if delta < MINUTE {
            return "in under a minute".to_owned();
        } else if round(MINUTE) < 60 {
            (round(MINUTE), "minute")
        } else if round(HOUR) < 24 {
            (round(HOUR), "hour")
        } else if round(DAY) < 30 {
            (round(DAY), "day")
        } else if delta < YEAR {
            (round(MONTH), "month")
        } else {
            return format!("on {}", then.date());
        };
        let plural = if amount == 1 { "" } else { "s" };
        format!("in {amount} {unit}{plural}")
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.relative())
    }
}

#[cfg(test)]
mod tests;
