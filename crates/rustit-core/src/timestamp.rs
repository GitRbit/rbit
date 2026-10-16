//! A Unix timestamp (seconds) with human-friendly formatting helpers for templates.

use std::fmt;

use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339, macros::format_description};

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

    /// e.g. `3 hours ago`, falling back to an absolute date after a year.
    pub fn relative(self) -> String {
        Self::relative_between(self, Self::now())
    }

    fn relative_between(then: Self, now: Self) -> String {
        const MINUTE: i64 = 60;
        const HOUR: i64 = 60 * MINUTE;
        const DAY: i64 = 24 * HOUR;
        const MONTH: i64 = 30 * DAY;
        const YEAR: i64 = 365 * DAY;

        let delta = (now.0 - then.0).max(0);
        let (amount, unit) = match delta {
            d if d < MINUTE => return "just now".to_owned(),
            d if d < HOUR => (d / MINUTE, "minute"),
            d if d < DAY => (d / HOUR, "hour"),
            d if d < MONTH => (d / DAY, "day"),
            d if d < YEAR => (d / MONTH, "month"),
            _ => return format!("on {}", then.date()),
        };
        let plural = if amount == 1 { "" } else { "s" };
        format!("{amount} {unit}{plural} ago")
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.relative())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_formatting() {
        let now = Timestamp(1_000_000_000);
        assert_eq!(Timestamp::relative_between(Timestamp(now.0 - 5), now), "just now");
        assert_eq!(Timestamp::relative_between(Timestamp(now.0 - 60), now), "1 minute ago");
        assert_eq!(Timestamp::relative_between(Timestamp(now.0 - 7200), now), "2 hours ago");
    }
}
