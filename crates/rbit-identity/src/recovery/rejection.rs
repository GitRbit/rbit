//! User-facing refusals; the text is part of the compatibility surface.

use std::fmt;

use super::{EMAIL_UNAVAILABLE, MAX_PASSWORD_LEN};
use crate::AccountName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryRejection {
    EmailUnavailable,
    InvalidAddress,
    AlreadyVerified,
    Suspended(AccountName),
    OwnPassword,
    SameAsInitial,
    PasswordTooShort { min: usize },
    PasswordTooLong,
}

impl fmt::Display for RecoveryRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmailUnavailable => f.write_str(EMAIL_UNAVAILABLE),
            Self::InvalidAddress => f.write_str("enter a valid email address"),
            Self::AlreadyVerified => f.write_str("your email address is already verified"),
            Self::Suspended(name) => write!(f, "{name} is suspended; lift the suspension first"),
            Self::OwnPassword => f.write_str("change your own password from your account settings"),
            Self::SameAsInitial => f.write_str("choose a different password from your initial password"),
            Self::PasswordTooShort { min } => write!(f, "password must be at least {min} characters"),
            Self::PasswordTooLong => write!(f, "password must be at most {MAX_PASSWORD_LEN} characters"),
        }
    }
}
