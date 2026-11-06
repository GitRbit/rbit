//! Shared slug grammar and validation errors used by independent feature names.

/// Why a user-supplied name was rejected. The message is safe to show to end users.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("{kind} must not be empty")]
    Empty { kind: &'static str },
    #[error("{kind} must be at most {max} characters")]
    TooLong { kind: &'static str, max: usize },
    #[error("{kind} may only contain letters, digits, '-', '_' and '.'")]
    InvalidCharacter { kind: &'static str },
    #[error("{kind} must start and end with a letter or digit")]
    InvalidBoundary { kind: &'static str },
    #[error("\"{value}\" is reserved and cannot be used as a {kind}")]
    Reserved { kind: &'static str, value: String },
    #[error("that does not look like a valid email address")]
    InvalidEmail,
    #[error("{kind} must be a positive whole number")]
    NotANumber { kind: &'static str },
}

/// Shared rule for "slug" style names: `[A-Za-z0-9._-]`, alphanumeric at both ends,
/// and never a Git-meaningful suffix such as `.git` or `.lock`.
///
/// This checks byte length, does not trim or normalize the accepted value, and
/// preserves the legacy error precedence. Callers own their route-reserved names
/// and supply their reserved route list in lowercase; successful validation does not claim
/// uniqueness, repository authorization or filesystem containment by itself.
pub fn validate_slug(kind: &'static str, value: &str, max: usize, reserved: &[&str]) -> Result<(), NameError> {
    if value.is_empty() {
        return Err(NameError::Empty { kind });
    }
    if value.len() > max {
        return Err(NameError::TooLong { kind, max });
    }
    if !value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')) {
        return Err(NameError::InvalidCharacter { kind });
    }
    let bytes = value.as_bytes();
    if !bytes[0].is_ascii_alphanumeric() || !bytes[bytes.len() - 1].is_ascii_alphanumeric() {
        return Err(NameError::InvalidBoundary { kind });
    }
    let lower = value.to_ascii_lowercase();
    if reserved.contains(&lower.as_str()) || [".git", ".lock"].iter().any(|suffix| lower.ends_with(suffix)) || lower.contains("..") {
        return Err(NameError::Reserved { kind, value: value.to_owned() });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
