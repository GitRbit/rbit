//! Errors returned by the `SQLite` and key/value stores.

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("registration is closed on this instance")]
    RegistrationClosed,

    /// A uniqueness constraint was violated. The payload names the conflicting field
    /// (e.g. `"name"`, `"email"`) so callers can produce a precise message.
    #[error("{0} is already taken")]
    Conflict(&'static str),

    /// A stored value no longer satisfies domain validation (e.g. a hand-edited row).
    #[error("stored data is invalid: {0}")]
    Corrupt(String),

    #[error("database error: {0}")]
    Sqlite(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("key/value store error: {0}")]
    Kv(#[from] redb::Error),

    #[error("background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl StorageError {
    /// Maps a unique-constraint violation from an insert to [`StorageError::Conflict`] naming `field`.
    pub(crate) fn from_insert(error: sqlx::Error, field: &'static str) -> Self {
        if let sqlx::Error::Database(db) = &error
            && db.is_unique_violation()
        {
            return Self::Conflict(field);
        }
        Self::Sqlite(error)
    }
}

/// Converts any of redb's granular error types into [`StorageError`].
pub(crate) fn kv_error(error: impl Into<redb::Error>) -> StorageError {
    StorageError::Kv(error.into())
}
