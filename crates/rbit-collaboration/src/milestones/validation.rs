//! Existing Unicode/title, byte-counted description and calendar-date validation.

use super::{MilestoneFields, MilestoneInput};

/// Longest milestone title, in characters.
pub const MAX_MILESTONE_TITLE_CHARS: usize = 100;
/// Longest milestone description, in bytes.
pub const MAX_MILESTONE_DESCRIPTION_BYTES: usize = 10_000;

fn error(message: impl Into<String>) -> String {
    message.into()
}

/// Whether `value` is a real calendar day written `YYYY-MM-DD`.
pub fn is_calendar_day(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let number = |range: std::ops::Range<usize>| value.get(range).filter(|s| s.bytes().all(|b| b.is_ascii_digit()))?.parse::<u32>().ok();
    let (Some(year), Some(month), Some(day)) = (number(0..4), number(5..7), number(8..10)) else { return false };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year >= 1 && (1..=days).contains(&day)
}

pub fn milestone_fields(input: &MilestoneInput) -> Result<MilestoneFields, String> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(error("milestone title must not be empty"));
    }
    if title.chars().count() > MAX_MILESTONE_TITLE_CHARS {
        return Err(error(format!("milestone title must be at most {MAX_MILESTONE_TITLE_CHARS} characters")));
    }
    if title.chars().any(char::is_control) {
        return Err(error("milestone title must not contain control characters"));
    }
    let description = input.description.replace("\r\n", "\n");
    if description.len() > MAX_MILESTONE_DESCRIPTION_BYTES {
        return Err(error(format!("milestone description must be at most {MAX_MILESTONE_DESCRIPTION_BYTES} bytes")));
    }
    let due_on = input.due_on.trim();
    let due_on = if due_on.is_empty() {
        None
    } else if is_calendar_day(due_on) {
        Some(due_on.to_owned())
    } else {
        return Err(error("due date must be a day written YYYY-MM-DD"));
    };
    Ok(MilestoneFields { title: title.to_owned(), description, due_on })
}
