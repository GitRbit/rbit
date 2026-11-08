//! Stable wiki outcomes. Presentation maps each variant to a localized message.
use rbit_git::wikis::WikiObjectError;
use rbit_kernel::AdmissionError;
use rbit_storage::{
    StorageError,
    sqlite::wiki_mutations::{WikiMutationError, publish::WikiPublishError},
};
use rbit_wikis::{WikiContentError, WikiNameError, policy::WikiPolicyError};

use crate::ServiceError;

/// Why a wiki read or change did not happen. None of these published anything.
#[derive(Debug, thiserror::Error)]
pub enum WikiError {
    /// The repository, its wiki or the page is missing, or the viewer may not know it.
    #[error("not found")]
    NotFound,
    /// The viewer may read the wiki but not change it.
    #[error("forbidden")]
    Forbidden,
    /// The repository is archived: its wiki is read-only.
    #[error("archived")]
    Archived,
    /// The page changed (or was deleted) after the form was read.
    #[error("conflict")]
    Conflict,
    /// Another page already has the requested path.
    #[error("path taken")]
    PathTaken,
    /// The submitted page equals the current page.
    #[error("unchanged")]
    Unchanged,
    #[error("invalid title")]
    InvalidTitle(WikiNameError),
    #[error("invalid path")]
    InvalidPath(WikiNameError),
    #[error("invalid content")]
    InvalidContent(WikiContentError),
    /// Wiki work capacity is exhausted or another change is being finalized.
    #[error("unavailable")]
    Unavailable,
    #[error(transparent)]
    Internal(ServiceError),
}

impl From<StorageError> for WikiError {
    fn from(error: StorageError) -> Self {
        Self::Internal(error.into())
    }
}

impl From<WikiObjectError> for WikiError {
    fn from(error: WikiObjectError) -> Self {
        match error {
            WikiObjectError::Admission(AdmissionError::Busy | AdmissionError::Closed) => Self::Unavailable,
            other => Self::Internal(ServiceError::internal(other)),
        }
    }
}

impl From<WikiPolicyError> for WikiError {
    fn from(error: WikiPolicyError) -> Self {
        match error {
            WikiPolicyError::NotFound => Self::NotFound,
            WikiPolicyError::Forbidden => Self::Forbidden,
            WikiPolicyError::Archived => Self::Archived,
            WikiPolicyError::Conflict => Self::Conflict,
            WikiPolicyError::Pending | WikiPolicyError::VersionExhausted => Self::Unavailable,
        }
    }
}

impl From<WikiMutationError> for WikiError {
    fn from(error: WikiMutationError) -> Self {
        match error {
            WikiMutationError::Policy(policy) => policy.into(),
            WikiMutationError::Storage(storage) => storage.into(),
            WikiMutationError::Capacity | WikiMutationError::Exhausted => Self::Unavailable,
            other => Self::Internal(ServiceError::Internal(format!("wiki mutation refused: {other:?}").into())),
        }
    }
}

impl From<WikiPublishError> for WikiError {
    fn from(error: WikiPublishError) -> Self {
        match error {
            WikiPublishError::Policy(policy) => policy.into(),
            // Retries are exhausted by the caller before this conversion.
            WikiPublishError::Moved => Self::Unavailable,
            WikiPublishError::PathTaken => Self::PathTaken,
            WikiPublishError::Mutation(mutation) => mutation.into(),
        }
    }
}

impl From<WikiError> for ServiceError {
    fn from(error: WikiError) -> Self {
        match error {
            WikiError::NotFound => Self::NotFound,
            WikiError::Forbidden => Self::Forbidden,
            WikiError::Archived => Self::Archived,
            WikiError::Unavailable => Self::Unavailable { retry_after_seconds: 1 },
            WikiError::Internal(error) => error,
            other => Self::Invalid(other.to_string()),
        }
    }
}
