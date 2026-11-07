//! Exact legacy validation and normalization; SQL still enforces uniqueness.

use super::{LabelFields, LabelInput};

/// Most labels in one repository.
pub const MAX_LABELS_PER_REPO: u64 = 200;
/// Longest label name, in characters.
pub const MAX_LABEL_NAME_CHARS: usize = 50;
/// Longest label description, in characters.
pub const MAX_LABEL_DESCRIPTION_CHARS: usize = 100;

/// The labels "Create default labels" adds: name, colour, description.
pub const DEFAULT_LABELS: [(&str, &str, &str); 9] = [
    ("bug", "#d73a4a", "Something does not work as intended"),
    ("documentation", "#0075ca", "Docs need writing or fixing"),
    ("duplicate", "#cfd3d7", "Already reported elsewhere"),
    ("enhancement", "#a2eeef", "A new feature or an improvement"),
    ("good first issue", "#7057ff", "A gentle start for newcomers"),
    ("help wanted", "#008672", "Extra hands are welcome"),
    ("invalid", "#e4e669", "Not a real problem"),
    ("question", "#d876e3", "More information is needed"),
    ("wontfix", "#ffffff", "Will not be worked on"),
];

fn error(message: impl Into<String>) -> String {
    message.into()
}

/// `#rrggbb` (lower case) from `#RRGGBB` or `rrggbb`.
pub fn normalise_color(color: &str) -> Result<String, String> {
    let trimmed = color.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(error("colour must be six hexadecimal digits, like #d73a4a"));
    }
    Ok(format!("#{}", hex.to_ascii_lowercase()))
}

pub fn label_fields(input: &LabelInput) -> Result<LabelFields, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(error("label name must not be empty"));
    }
    if name.chars().count() > MAX_LABEL_NAME_CHARS {
        return Err(error(format!("label name must be at most {MAX_LABEL_NAME_CHARS} characters")));
    }
    if name.chars().any(char::is_control) {
        return Err(error("label name must not contain control characters"));
    }
    let description = input.description.trim();
    if description.chars().count() > MAX_LABEL_DESCRIPTION_CHARS {
        return Err(error(format!("label description must be at most {MAX_LABEL_DESCRIPTION_CHARS} characters")));
    }
    if description.chars().any(char::is_control) {
        return Err(error("label description must not contain control characters"));
    }
    Ok(LabelFields { name: name.to_owned(), color: normalise_color(&input.color)?, description: description.to_owned() })
}
