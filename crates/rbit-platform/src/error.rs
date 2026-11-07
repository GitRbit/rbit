use std::{fmt, io};

/// Fixed contexts keep private file names/content out of normal error formatting.
pub enum PlatformError {
    InvalidName,
    Unsupported,
    Busy,
    Changed,
    Conflict,
    /// Native completion is unknown. Exactly one real subject lease is retained.
    Quarantined,
    Io(io::Error),
}
impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidName => "invalid generated filesystem component",
            Self::Unsupported => "unsupported filesystem mode",
            Self::Busy => "filesystem subject already has an owner",
            Self::Changed => "filesystem binding changed",
            Self::Conflict => "filesystem destination already exists",
            Self::Quarantined => "native completion unproved; process restart required",
            Self::Io(_) => "filesystem operation failed",
        })
    }
}
impl fmt::Debug for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl std::error::Error for PlatformError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}
impl From<io::Error> for PlatformError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
