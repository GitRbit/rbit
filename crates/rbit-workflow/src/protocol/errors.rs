//! Protocol errors contracts and original validation.
use super::limits::SUPPORTED_PROTOCOL_VERSIONS;
use super::validation::ValidationError;
use serde::{Deserialize, Serialize};

/// The `code` of an [`crate::protocol::ErrorBody`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The body is not a valid message ([`crate::protocol::ValidationError`], bad JSON, a bad query).
    BadRequest,
    /// No or an unknown credential or token.
    Unauthenticated,
    /// Authenticated, but not allowed (another runner's task, a pinned label).
    Forbidden,
    /// The task or runner is gone; a runner cancels the task locally.
    NotFound,
    /// The call contradicts the stored state (a log gap: see [`crate::protocol::ErrorBody::next_line`]).
    Conflict,
    /// The body exceeds its cap or the gzip ratio.
    TooLarge,
    /// The protocol version is not served; see [`crate::protocol::ErrorBody::supported_versions`].
    UnsupportedProtocol,
    /// Too many calls; the `Retry-After` header says when to try again.
    Throttled,
    /// The server cannot take the call now (too many polls, shutting down).
    Unavailable,
    /// A code from a newer server. Treated like [`crate::protocol::ErrorCode::Unavailable`].
    #[serde(other)]
    Unknown,
}

impl ErrorCode {
    /// The HTTP status the code is sent with.
    pub const fn status(self) -> u16 {
        match self {
            Self::BadRequest => 400,
            Self::Unauthenticated => 401,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::Conflict => 409,
            Self::TooLarge => 413,
            Self::UnsupportedProtocol => 426,
            Self::Throttled => 429,
            Self::Unavailable | Self::Unknown => 503,
        }
    }
}

/// The body of every error answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    /// With [`crate::protocol::ErrorCode::UnsupportedProtocol`]: the versions the server speaks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_versions: Vec<u32>,
    /// With [`crate::protocol::ErrorCode::Conflict`] on a log append: the line to resend from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_line: Option<u64>,
}

impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), supported_versions: Vec::new(), next_line: None }
    }

    pub fn unsupported_protocol() -> Self {
        Self {
            supported_versions: SUPPORTED_PROTOCOL_VERSIONS.to_vec(),
            ..Self::new(ErrorCode::UnsupportedProtocol, "this server does not speak the runner's protocol version")
        }
    }

    pub fn log_gap(next_line: u64) -> Self {
        Self { next_line: Some(next_line), ..Self::new(ErrorCode::Conflict, format!("the log continues at line {next_line}")) }
    }
}

impl From<ValidationError> for ErrorBody {
    fn from(error: ValidationError) -> Self {
        Self::new(ErrorCode::BadRequest, error.message)
    }
}
