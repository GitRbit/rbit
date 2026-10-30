//! Errors returned by application services.

use rbit_core::{NameError, issue::IssueTextError};
use rbit_storage::StorageError;

/// Outcomes a caller must handle. Messages of the user-facing variants are safe to display.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    /// Input failed validation; the message explains how to fix it.
    #[error("{0}")]
    Invalid(String),

    /// A unique value (username, email, repository name) is already in use.
    #[error("{0} is already taken")]
    Conflict(&'static str),

    /// Wrong username/password. Deliberately does not say which.
    #[error("incorrect username or password")]
    InvalidCredentials,

    #[error("registration is closed on this instance")]
    RegistrationClosed,

    /// The resource does not exist *or* the actor may not know it exists.
    #[error("not found")]
    NotFound,

    /// The actor may see the resource but not perform this action.
    #[error("you do not have permission to do that")]
    Forbidden,

    /// The repository is archived, so nothing in it may change (see
    /// [`crate::RepoContext::ensure_writable`]).
    #[error("{}", rbit_core::repository::ARCHIVED_READ_ONLY)]
    Archived,

    /// Too many attempts from this client or against this account; retry later.
    #[error("too many attempts; try again in {retry_after_seconds} seconds")]
    RateLimited { retry_after_seconds: u64 },

    /// Anything unexpected. Logged in full, shown to users generically.
    #[error("internal error: {0}")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl ServiceError {
    /// Wraps an implementation error for host diagnostics and generic error responses.
    pub fn internal(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Internal(Box::new(error))
    }
}

impl From<StorageError> for ServiceError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::RegistrationClosed => Self::RegistrationClosed,
            StorageError::Conflict(field) => Self::Conflict(field),
            other => Self::internal(other),
        }
    }
}

impl From<NameError> for ServiceError {
    fn from(error: NameError) -> Self {
        Self::Invalid(error.to_string())
    }
}

impl From<IssueTextError> for ServiceError {
    fn from(error: IssueTextError) -> Self {
        Self::Invalid(error.to_string())
    }
}

impl From<rbit_git::GitError> for ServiceError {
    fn from(error: rbit_git::GitError) -> Self {
        match error {
            rbit_git::GitError::NotFound(_) => Self::NotFound,
            other => Self::internal(other),
        }
    }
}

impl From<rbit_auth::PasswordError> for ServiceError {
    fn from(error: rbit_auth::PasswordError) -> Self {
        Self::internal(error)
    }
}
