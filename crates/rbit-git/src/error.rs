//! Errors returned by repository storage and Git operations.

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// A revision, path or object the caller asked for does not exist.
    #[error("{0} not found")]
    NotFound(&'static str),

    /// The repository has no commits yet.
    #[error("repository is empty")]
    Empty,

    #[error("invalid branch name: {0}")]
    InvalidBranchName(String),

    #[error("repository already exists on disk")]
    AlreadyExists,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Any error reported by gitoxide. Its many granular error types are boxed here
    /// because callers only ever log them.
    #[error("git backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl GitError {
    pub(crate) fn backend(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Backend(Box::new(error))
    }
}
