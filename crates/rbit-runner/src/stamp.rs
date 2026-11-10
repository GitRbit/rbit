//! Log line times: every line the runner logs starts with the UTC time it was written,
//! `2026-11-10T12:00:01.123Z `, as GitHub's raw logs do. The job page shows them on
//! request and the raw log keeps them. Echoed lines (`rbit-runner exec`) go without.

use std::time::{SystemTime, UNIX_EPOCH};

/// Bytes a time takes before a line, its space included.
pub const LENGTH: usize = 25;

/// `at` (milliseconds since the epoch) as `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub fn format(at: u64) -> String {
    let (seconds, millis) = (at / 1000, at % 1000);
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let rest = seconds % 86_400;
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

/// `text`, each of its lines starting with the time now.
pub fn lines(text: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(0));
    let time = format(now);
    let mut out = String::with_capacity(text.len() + LENGTH);
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&time);
        out.push(' ');
        out.push_str(line);
    }
    out
}

/// `line` without its time, for tests that compare what steps printed.
#[cfg(test)]
pub fn strip(line: &str) -> &str {
    match line.get(..LENGTH) {
        Some(head) if head.ends_with("Z ") && head.as_bytes().get(10) == Some(&b'T') => &line[LENGTH..],
        _ => line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_utc_with_milliseconds() {
        assert_eq!(format(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(format(1_762_776_001_123), "2025-11-10T12:00:01.123Z");
        assert_eq!(format(951_782_400_000), "2000-02-29T00:00:00.000Z");
        assert_eq!(format(4_107_542_399_999), "2100-02-28T23:59:59.999Z");
    }

    #[test]
    fn every_line_is_stamped_and_strips_back() {
        let stamped = lines("one\ntwo");
        let parts: Vec<&str> = stamped.split('\n').collect();
        assert_eq!(parts.len(), 2);
        assert!(parts.iter().all(|part| part.len() > LENGTH && part.as_bytes()[LENGTH - 2] == b'Z'), "{stamped}");
        assert_eq!(parts.iter().map(|part| strip(part)).collect::<Vec<_>>(), ["one", "two"]);
        assert_eq!(strip("plain"), "plain");
    }
}
