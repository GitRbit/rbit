//! User-facing refusals; the text is part of the compatibility surface.

use std::fmt;

use super::{MAX_PASSKEY_NAME, MAX_PASSKEYS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasskeyRejection {
    NoPublicUrl,
    NameTooLong,
    NameControlCharacters,
    RequestExpired,
    TooMany,
    ResponseTooLarge,
    NotVerified,
    AlreadyRegistered,
}

impl fmt::Display for PasskeyRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoPublicUrl => f.write_str("passkeys need `server.public_url` to be set to this site's address"),
            Self::NameTooLong => write!(f, "the name can be at most {MAX_PASSKEY_NAME} characters"),
            Self::NameControlCharacters => f.write_str("the name can't contain control characters"),
            Self::RequestExpired => f.write_str("the passkey request expired; try again"),
            Self::TooMany => write!(f, "you can register at most {MAX_PASSKEYS} passkeys"),
            Self::ResponseTooLarge => f.write_str("the passkey response is too large"),
            Self::NotVerified => f.write_str("the passkey couldn't be verified; try again"),
            Self::AlreadyRegistered => f.write_str("that passkey is already registered"),
        }
    }
}
