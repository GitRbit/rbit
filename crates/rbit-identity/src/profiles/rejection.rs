//! User-facing refusals; the text is part of the compatibility surface.

use std::fmt;

use super::MAX_PASSWORD_LEN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileRejection {
    NotSingleLine { field: &'static str },
    TooLong { field: &'static str, max: usize },
    InvalidCharacters { field: &'static str },
    WebsiteNotHttp,
    PasswordTooShort { min: usize },
    PasswordTooLong,
    SamePassword,
}

impl fmt::Display for ProfileRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSingleLine { field } => write!(f, "{field} must be a single line of text"),
            Self::TooLong { field, max } => write!(f, "{field} must be at most {max} characters"),
            Self::InvalidCharacters { field } => write!(f, "{field} contains invalid characters"),
            Self::WebsiteNotHttp => f.write_str("website must be a URL starting with http:// or https://"),
            Self::PasswordTooShort { min } => write!(f, "new password must be at least {min} characters"),
            Self::PasswordTooLong => write!(f, "new password must be at most {MAX_PASSWORD_LEN} characters"),
            Self::SamePassword => f.write_str("choose a different password from your current password"),
        }
    }
}
