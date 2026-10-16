use rustit_core::NameError;
use rustit_storage::StorageError;

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

    /// Anything unexpected. Logged in full, shown to users generically.
    #[error("internal error: {0}")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl ServiceError {
    pub(crate) fn internal(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Internal(Box::new(error))
    }
}

impl From<StorageError> for ServiceError {
    fn from(error: StorageError) -> Self {
        match error {
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

impl From<rustit_git::GitError> for ServiceError {
    fn from(error: rustit_git::GitError) -> Self {
        match error {
            rustit_git::GitError::NotFound(_) => Self::NotFound,
            other => Self::internal(other),
        }
    }
}

impl From<rustit_auth::PasswordError> for ServiceError {
    fn from(error: rustit_auth::PasswordError) -> Self {
        Self::internal(error)
    }
}
