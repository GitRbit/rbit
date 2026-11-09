//! User-facing refusals; the text is part of the compatibility surface.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TwoFactorRejection {
    AlreadySetUp,
    NoSetup,
    RestartSetup,
    SetupChanged,
    NotEnabled,
}

impl fmt::Display for TwoFactorRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::AlreadySetUp => "an authenticator app is already set up",
            Self::NoSetup => "there is no authenticator setup in progress; start again",
            Self::RestartSetup => "start the setup again",
            Self::SetupChanged => "the authenticator setup changed; start again",
            Self::NotEnabled => "two-factor authentication is not enabled",
        })
    }
}
