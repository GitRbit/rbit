//! `schedule` expressions: five-field POSIX cron in UTC, with the semantics of
//! `robfig/cron`'s standard parser (which Forgejo and Gitea use), so a schedule fires at the
//! same minutes as there.
//!
//! * Fields: minute (0–59), hour (0–23), day of month (1–31), month (1–12 or `JAN`–`DEC`),
//!   day of week (0–6, Sunday first, or `SUN`–`SAT`); names are case-insensitive.
//! * Each field is a comma-separated list of `*`, `N`, `N-M`, each optionally `/step`
//!   (`N/step` means `N-max/step`); `?` is `*` in the two day fields.
//! * Days: when both day fields are restricted, a day matches if **either** does; when one
//!   of them is `*` (or `?`, or `*/1`), both must match — robfig's rule.
//! * `@yearly` (`@annually`), `@monthly`, `@weekly`, `@daily` (`@midnight`), `@hourly`.
//!
//! [`Cron::next_after`] computes the next fire time with plain calendar arithmetic; there
//! is no time-zone database and no daylight saving time to account for.

use std::fmt;

/// The shortest interval between two runs of one schedule that GitHub (and rbit) accept.
pub const MIN_INTERVAL_MINUTES: u32 = 5;

/// How far ahead [`Cron::next_after`] looks before concluding a schedule never fires
/// (`0 0 30 2 *`). Eight years always contain a 29 February.
const SEARCH_YEARS: i64 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronError(String);

impl fmt::Display for CronError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CronError {}

fn invalid(message: impl Into<String>) -> CronError {
    CronError(message.into())
}

/// A parsed schedule. Each field is a bit set of the values it allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    source: String,
    minutes: u64,
    hours: u64,
    days: u64,
    months: u64,
    weekdays: u64,
    /// The day-of-month or day-of-week field was unrestricted (`*`, `?`, `*/1`).
    day_star: bool,
}

struct Field {
    name: &'static str,
    min: u32,
    max: u32,
    names: &'static [&'static str],
    /// `?` stands for `*` (the two day fields).
    question: bool,
}

const MINUTE: Field = Field { name: "minute", min: 0, max: 59, names: &[], question: false };
const HOUR: Field = Field { name: "hour", min: 0, max: 23, names: &[], question: false };
const DAY: Field = Field { name: "day of month", min: 1, max: 31, names: &[], question: true };
const MONTH: Field = Field {
    name: "month",
    min: 1,
    max: 12,
    names: &["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"],
    question: false,
};
const WEEKDAY: Field =
    Field { name: "day of week", min: 0, max: 6, names: &["sun", "mon", "tue", "wed", "thu", "fri", "sat"], question: true };

impl Cron {
    pub fn parse(expression: &str) -> Result<Self, CronError> {
        let source = expression.trim();
        let expanded = match source.to_ascii_lowercase().as_str() {
            "@yearly" | "@annually" => "0 0 1 1 *",
            "@monthly" => "0 0 1 * *",
            "@weekly" => "0 0 * * 0",
            "@daily" | "@midnight" => "0 0 * * *",
            "@hourly" => "0 * * * *",
            other if other.starts_with('@') => return Err(invalid(format!("`{source}` is not a supported schedule"))),
            _ => source,
        };
        let fields: Vec<&str> = expanded.split_whitespace().collect();
        let [minute, hour, day, month, weekday] = fields[..] else {
            return Err(invalid(format!("`{source}` must have five fields: minute, hour, day of month, month and day of week")));
        };
        let (minutes, _) = parse_field(minute, &MINUTE)?;
        let (hours, _) = parse_field(hour, &HOUR)?;
        let (days, day_star) = parse_field(day, &DAY)?;
        let (months, _) = parse_field(month, &MONTH)?;
        let (weekdays, weekday_star) = parse_field(weekday, &WEEKDAY)?;
        Ok(Self { source: source.to_owned(), minutes, hours, days, months, weekdays, day_star: day_star || weekday_star })
    }

    /// The expression as written.
    pub fn as_str(&self) -> &str {
        &self.source
    }

    /// Whether two runs could be less than [`MIN_INTERVAL_MINUTES`] apart. Exact within a
    /// day; across midnight it assumes consecutive days can both fire.
    pub fn is_too_frequent(&self) -> bool {
        let fires: Vec<u32> = bits(self.hours).flat_map(|hour| bits(self.minutes).map(move |minute| hour * 60 + minute)).collect();
        let (Some(&first), Some(&last)) = (fires.first(), fires.last()) else {
            return false;
        };
        let across_midnight = 24 * 60 - last + first;
        fires.windows(2).map(|pair| pair[1] - pair[0]).chain([across_midnight]).any(|gap| gap < MIN_INTERVAL_MINUTES)
    }

    /// The first fire time strictly after `after` (Unix seconds, UTC), or `None` when the
    /// schedule never fires (`0 0 31 2 *`).
    pub fn next_after(&self, after: i64) -> Option<i64> {
        let mut minute = after.div_euclid(60) + 1;
        let limit = minute + SEARCH_YEARS * 366 * 24 * 60;
        while minute < limit {
            let days = minute.div_euclid(24 * 60);
            let (year, month, day) = civil_from_days(days);
            if !has(self.months, month) {
                // The first minute of the next month.
                let (year, month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
                minute = days_from_civil(year, month, 1) * 24 * 60;
                continue;
            }
            if !self.day_matches(day, weekday(days)) {
                minute = (days + 1) * 24 * 60;
                continue;
            }
            let of_day = u32::try_from(minute.rem_euclid(24 * 60)).unwrap_or(0);
            let (hour, of_hour) = (of_day / 60, of_day % 60);
            if !has(self.hours, hour) {
                minute = days * 24 * 60 + i64::from(hour + 1) * 60;
                continue;
            }
            if !has(self.minutes, of_hour) {
                minute += 1;
                continue;
            }
            return Some(minute * 60);
        }
        None
    }

    fn day_matches(&self, day: u32, weekday: u32) -> bool {
        let (day, weekday) = (has(self.days, day), has(self.weekdays, weekday));
        if self.day_star { day && weekday } else { day || weekday }
    }
}

impl fmt::Display for Cron {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.source)
    }
}

fn has(set: u64, value: u32) -> bool {
    set & (1 << value) != 0
}

fn bits(set: u64) -> impl Iterator<Item = u32> {
    (0..64).filter(move |&bit| has(set, bit))
}

/// One field: its bit set, and whether it was unrestricted (robfig's "star bit": a part
/// that starts with `*` or `?` and has no step above 1).
fn parse_field(text: &str, field: &Field) -> Result<(u64, bool), CronError> {
    let mut set = 0u64;
    let mut star = false;
    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => {
                let step: u32 = step
                    .parse()
                    .ok()
                    .filter(|&step| step > 0)
                    .ok_or_else(|| invalid(format!("the {} field has an invalid step in `{part}`", field.name)))?;
                (range, Some(step))
            }
            None => (part, None),
        };
        let unrestricted = range == "*" || range == "?" && field.question;
        let (start, end) = if unrestricted {
            (field.min, field.max)
        } else if let Some((start, end)) = range.split_once('-') {
            (value(start, field)?, value(end, field)?)
        } else {
            let start = value(range, field)?;
            // `N/step` runs to the end of the field.
            (start, if step.is_some() { field.max } else { start })
        };
        if start > end {
            return Err(invalid(format!("the {} field has a range that runs backwards in `{part}`", field.name)));
        }
        let step = step.unwrap_or(1);
        star |= unrestricted && step == 1;
        for value in (start..=end).step_by(step as usize) {
            set |= 1 << value;
        }
    }
    Ok((set, star))
}

fn value(text: &str, field: &Field) -> Result<u32, CronError> {
    let lower = text.to_ascii_lowercase();
    let value = match field.names.iter().position(|name| *name == lower) {
        Some(index) => u32::try_from(index).unwrap_or(0) + field.min,
        None => text.parse::<u32>().map_err(|_| invalid(format!("the {} field has an invalid value `{text}`", field.name)))?,
    };
    if !(field.min..=field.max).contains(&value) {
        return Err(invalid(format!("the {} field must be between {} and {}, not {value}", field.name, field.min, field.max)));
    }
    Ok(value)
}

/// Days since 1970-01-01 → (year, month 1–12, day 1–31). Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// (year, month 1–12, day 1–31) → days since 1970-01-01.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let month = i64::from(month);
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// 0 = Sunday. 1970-01-01 was a Thursday.
fn weekday(days: i64) -> u32 {
    u32::try_from((days + 4).rem_euclid(7)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unix seconds of a UTC date and time.
    fn at(year: i64, month: u32, day: u32, hour: i64, minute: i64) -> i64 {
        days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60
    }

    #[test]
    fn calendar_arithmetic() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        for days in (-800_000..800_000).step_by(997) {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), days);
        }
        assert_eq!(weekday(0), 4);
        assert_eq!(weekday(days_from_civil(2026, 10, 23)), 5, "a Friday");
        assert_eq!(at(2026, 10, 23, 12, 30), 1_792_758_600);
    }

    #[test]
    fn next_fire_times() {
        /// Year, month, day, hour, minute.
        type Moment = (i64, u32, u32, i64, i64);
        let cases: &[(&str, Moment, Moment)] = &[
            ("*/5 * * * *", (2026, 10, 23, 12, 0), (2026, 10, 23, 12, 5)),
            ("*/5 * * * *", (2026, 10, 23, 12, 3), (2026, 10, 23, 12, 5)),
            ("0 * * * *", (2026, 10, 23, 23, 0), (2026, 10, 24, 0, 0)),
            ("30 2 * * *", (2026, 10, 23, 2, 30), (2026, 10, 24, 2, 30)),
            ("0 0 1 * *", (2026, 12, 15, 0, 0), (2027, 1, 1, 0, 0)),
            ("0 0 31 * *", (2026, 4, 1, 0, 0), (2026, 5, 31, 0, 0)),
            ("0 0 29 2 *", (2026, 1, 1, 0, 0), (2028, 2, 29, 0, 0)),
            ("0 0 28-31 2 *", (2027, 2, 28, 0, 0), (2028, 2, 28, 0, 0)),
            ("59 23 31 12 *", (2026, 12, 31, 23, 58), (2026, 12, 31, 23, 59)),
            ("0 12 * * MON-FRI", (2026, 10, 23, 12, 0), (2026, 10, 26, 12, 0)),
            ("0 12 * * sun", (2026, 10, 23, 0, 0), (2026, 10, 25, 12, 0)),
            ("0 9 1 * 1", (2026, 10, 23, 0, 0), (2026, 10, 26, 9, 0)),
            ("0 9 1 * 1", (2026, 10, 26, 9, 0), (2026, 11, 1, 9, 0)),
            ("0 9 1,15 * *", (2026, 10, 2, 0, 0), (2026, 10, 15, 9, 0)),
            ("0 9 */10 * *", (2026, 10, 2, 0, 0), (2026, 10, 11, 9, 0)),
            ("0 9 * * */2", (2026, 10, 23, 0, 0), (2026, 10, 24, 9, 0)),
            ("0 0 * * 0", (2026, 10, 23, 0, 0), (2026, 10, 25, 0, 0)),
            ("15 10 * JUN,dec *", (2026, 10, 23, 0, 0), (2026, 12, 1, 10, 15)),
            ("0 0 1 1 *", (2026, 1, 1, 0, 0), (2027, 1, 1, 0, 0)),
            ("@yearly", (2026, 1, 1, 0, 0), (2027, 1, 1, 0, 0)),
            ("@monthly", (2026, 1, 31, 0, 0), (2026, 2, 1, 0, 0)),
            ("@weekly", (2026, 10, 25, 0, 0), (2026, 11, 1, 0, 0)),
            ("@daily", (2026, 10, 23, 0, 0), (2026, 10, 24, 0, 0)),
            ("@hourly", (2026, 10, 23, 0, 59), (2026, 10, 23, 1, 0)),
            ("5-10/5 1 * * *", (2026, 10, 23, 1, 5), (2026, 10, 23, 1, 10)),
            ("7/20 * * * *", (2026, 10, 23, 1, 47), (2026, 10, 23, 2, 7)),
        ];
        for (expression, (y, mo, d, h, mi), (ey, emo, ed, eh, emi)) in cases {
            let cron = Cron::parse(expression).unwrap();
            assert_eq!(cron.next_after(at(*y, *mo, *d, *h, *mi)), Some(at(*ey, *emo, *ed, *eh, *emi)), "{expression}");
            // Seconds within the minute do not matter.
            assert_eq!(cron.next_after(at(*y, *mo, *d, *h, *mi) + 59), Some(at(*ey, *emo, *ed, *eh, *emi)), "{expression}");
        }
        assert_eq!(Cron::parse("0 0 30 2 *").unwrap().next_after(0), None);
        assert_eq!(Cron::parse("0 0 31 4,6,9,11 *").unwrap().next_after(0), None);
    }

    #[test]
    fn invalid_expressions() {
        for expression in [
            "",
            "* * * *",
            "* * * * * *",
            "60 * * * *",
            "* 24 * * *",
            "* * 0 * *",
            "* * 32 * *",
            "* * * 13 *",
            "* * * * 7",
            "5-1 * * * *",
            "*/0 * * * *",
            "*/x * * * *",
            "a * * * *",
            "* * * JANUARY *",
            "1,,2 * * * *",
            "? * * * *",
            "@every 5m",
            "@reboot",
            "TZ=UTC 0 0 * * *",
        ] {
            assert!(Cron::parse(expression).is_err(), "{expression}");
        }
        assert!(Cron::parse("60 * * * *").unwrap_err().to_string().contains("between 0 and 59"));
    }

    #[test]
    fn question_mark_only_in_day_fields() {
        let cron = Cron::parse("0 0 ? * MON").unwrap();
        assert!(cron.day_star);
        assert_eq!(cron.next_after(at(2026, 10, 23, 0, 0)), Some(at(2026, 10, 26, 0, 0)));
        assert!(Cron::parse("0 0 1 * ?").is_ok());
        assert!(Cron::parse("0 ? * * *").is_err());
    }

    #[test]
    fn minimum_interval() {
        for frequent in ["* * * * *", "*/4 * * * *", "0,3 * * * *", "0-4 9 * * *", "0,59 * * * *", "0,57 * * * *"] {
            assert!(Cron::parse(frequent).unwrap().is_too_frequent(), "{frequent}");
        }
        for fine in ["*/5 * * * *", "0 * * * *", "0,30 * * * *", "@hourly", "@daily", "0 9 * * 1-5", "58 23,0 * * *", "58 * * * *"] {
            assert!(!Cron::parse(fine).unwrap().is_too_frequent(), "{fine}");
        }
    }
}
