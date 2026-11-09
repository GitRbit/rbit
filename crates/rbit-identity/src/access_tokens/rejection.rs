//! User-facing refusals; the text is part of the compatibility surface.

use std::fmt;

use super::{MAX_NAME_LEN, MAX_TOKENS_PER_ACCOUNT};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessTokenRejection {
    EmptyName,
    NameTooLong,
    UnofferedExpiry,
    LimitReached,
}

impl fmt::Display for AccessTokenRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => f.write_str("token name must not be empty"),
            Self::NameTooLong => write!(f, "token name must be at most {MAX_NAME_LEN} characters"),
            Self::UnofferedExpiry => f.write_str("choose one of the offered expiration periods"),
            Self::LimitReached => {
                write!(f, "you can have at most {MAX_TOKENS_PER_ACCOUNT} access tokens; revoke one you no longer use")
            }
        }
    }
}
