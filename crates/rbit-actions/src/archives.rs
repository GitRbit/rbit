//! What the server's artifact and shared-cache stores have in common (§12.5.6, §12.7):
//! each keeps one tar.gz per row, received into a temporary file beside the store while
//! its size and SHA-256 are counted, and moved into place only once its row is recorded.
//!
//! The ports are static and borrowed: composition implements them over its database and
//! file system. No I/O, runtime or clock lives here; times are Unix seconds the ports give.
//! Refusals carry the exact text a runner sees; port errors pass through unchanged.

use std::{fmt, future::Future, time::Duration};

/// Seconds in a retention day.
pub const SECONDS_PER_DAY: i64 = 86_400;

/// Most rows one sweep pass expires, and one cache save evicts.
pub const BATCH: u32 = 256;

/// A stored file without its row is removed once it is this old: a younger one may belong
/// to a row on its way in.
pub const ORPHAN_AGE: Duration = Duration::from_hours(1);

/// An unfinished upload is removed once it is this old.
pub const STALE_UPLOAD_AGE: Duration = Duration::from_hours(24);

/// The directory of a store that holds uploads on their way in.
pub const UPLOADS_DIRECTORY: &str = "tmp";

/// The cutoff `days` retention days before `now`.
pub fn days_before(now: i64, days: u32) -> i64 {
    now - i64::from(days) * SECONDS_PER_DAY
}

/// A directory below the artifact store: uploads on their way in, or one run's files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactDirectory {
    Uploads,
    Run(i64),
    /// Not the store's; left alone.
    Other,
}

impl ArtifactDirectory {
    /// What directory `name` below the artifact store holds.
    pub fn named(name: &str) -> Self {
        if name == UPLOADS_DIRECTORY {
            Self::Uploads
        } else if let Ok(run) = name.parse() {
            Self::Run(run)
        } else {
            Self::Other
        }
    }
}

/// The cache entry a stored file `<id>.tar.gz` belongs to, if it names one.
pub fn cache_entry_file(name: &str) -> Option<i64> {
    name.strip_suffix(".tar.gz")?.parse().ok()
}

/// An upload received into its temporary file.
#[derive(Debug)]
pub struct Received<T> {
    pub temporary: T,
    pub size: u64,
    pub sha256: [u8; 32],
}

/// The stored files of a store.
pub trait ArchiveFiles: Sync {
    type Error: Send;
    type Location: Send + Sync;

    /// Removes a stored file; its absence is no error, and the port logs other failures.
    fn remove(&self, location: &Self::Location) -> impl Future<Output = ()> + Send;
}

/// Uploads into a store.
pub trait ArchiveUploads: ArchiveFiles {
    type Body: Send;
    type Temporary: Send + Sync;

    /// Streams `body` into a new temporary file of the store, counting its size and SHA-256;
    /// `None` once it passes `limit` bytes. The port removes a partial file itself, also
    /// when receiving fails.
    fn receive(&self, body: Self::Body, limit: u64) -> impl Future<Output = Result<Option<Received<Self::Temporary>>, Self::Error>> + Send;
    /// Removes a temporary file, ignoring failures.
    fn discard(&self, temporary: &Self::Temporary) -> impl Future<Output = ()> + Send;
    /// Moves a temporary file to `location`, creating its directory.
    fn place(&self, temporary: &Self::Temporary, location: &Self::Location) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// Why a store refused a request, with the text the runner sees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    BadRequest(String),
    TooLarge(String),
    Conflict(String),
    NotFound(String),
}

/// A refusal, or what a port failed with.
pub enum StoreError<E> {
    Refused(Refusal),
    Port(E),
}

impl<E> StoreError<E> {
    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self::Refused(Refusal::BadRequest(message.into()))
    }
}

// Port errors may not implement Debug; diagnostics never format them.
impl<E> fmt::Debug for StoreError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(refusal) => f.debug_tuple("Refused").field(refusal).finish(),
            Self::Port(_) => f.write_str("Port(..)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_directories_and_files_are_named_as_stored() {
        assert_eq!(ArtifactDirectory::named("tmp"), ArtifactDirectory::Uploads);
        assert_eq!(ArtifactDirectory::named("42"), ArtifactDirectory::Run(42));
        assert_eq!(ArtifactDirectory::named("x"), ArtifactDirectory::Other);
        assert_eq!(cache_entry_file("7.tar.gz"), Some(7));
        assert_eq!(cache_entry_file("7.part"), None);
        assert_eq!(cache_entry_file("x.tar.gz"), None);
        assert_eq!(days_before(1_000_000, 2), 1_000_000 - 172_800);
    }
}
