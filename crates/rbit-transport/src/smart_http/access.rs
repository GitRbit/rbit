//! Whether a smart HTTP request may use a repository, and what Git is told when not.
//!
//! The checks run in a fixed order: open the repository for the caller, then (pushes
//! only) write access, then whether it is a pull mirror. The mirror check comes last so
//! that only callers who could otherwise push learn that the repository is a mirror.
//! Anonymous callers are challenged for credentials instead of receiving a definitive
//! "not found" or "forbidden", so Git prompts rather than fails.
//!
//! Opening (including old names from before a rename or transfer, task grants and the
//! cap a personal access token's scope puts on access) is the
//! [`RepositoryGate`] adapter's job; this module never touches storage.

use std::{fmt, future::Future};

use crate::Service;

/// The body of a push refused for missing write access.
pub const PUSH_FORBIDDEN: &str = "you do not have permission to push to this repository";

/// The body of a request for a repository the caller cannot see.
pub const REPOSITORY_NOT_FOUND: &str = "repository not found";

/// Repository lookups for one authenticated (or anonymous) caller.
pub trait RepositoryGate {
    /// The opened repository, with the caller's access already capped by its credential.
    type Repository: Send;
    /// A failure to look something up (never "not found").
    type Error: fmt::Display + Send;

    /// Opens `owner/name` for the caller. `Ok(None)` when it does not exist or the caller
    /// may not see it; the two are indistinguishable on purpose.
    fn open(&self, owner: &str, name: &str) -> impl Future<Output = Result<Option<Self::Repository>, Self::Error>> + Send;

    /// Whether the caller's access to `repository` allows pushing.
    fn can_write(&self, repository: &Self::Repository) -> bool;

    /// Whether `repository` is a pull mirror (which only changes by fetching upstream).
    fn is_mirror(&self, repository: &Self::Repository) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

/// Why access was refused; the route renders each one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessRefusal {
    /// `401` with [`super::credentials::BASIC_CHALLENGE`]: ask for (better) credentials.
    Challenge,
    /// `403` [`PUSH_FORBIDDEN`].
    Forbidden,
    /// `404` [`REPOSITORY_NOT_FOUND`].
    NotFound,
    /// `403`: a push to a pull mirror.
    Mirror,
    /// `500`; the cause has been logged.
    Internal,
}

/// Opens the repository for `service`, refusing as Git over HTTP always has.
/// `anonymous` is whether the request carried no credential.
pub async fn authorize<G>(gate: &G, anonymous: bool, owner: &str, name: &str, service: Service) -> Result<G::Repository, AccessRefusal>
where
    G: RepositoryGate + Sync,
{
    let repository = match gate.open(owner, name).await {
        Ok(Some(repository)) => repository,
        // Anonymous visitors get a credential prompt instead of a definitive answer.
        Ok(None) if anonymous => return Err(AccessRefusal::Challenge),
        Ok(None) => return Err(AccessRefusal::NotFound),
        Err(error) => {
            tracing::error!(%error, "git authorization failed");
            return Err(AccessRefusal::Internal);
        }
    };
    if !service.is_write() {
        return Ok(repository);
    }
    if !gate.can_write(&repository) {
        return Err(if anonymous { AccessRefusal::Challenge } else { AccessRefusal::Forbidden });
    }
    // Checked after access, so only those who could otherwise push learn it is a mirror.
    match gate.is_mirror(&repository).await {
        Ok(false) => Ok(repository),
        Ok(true) => Err(AccessRefusal::Mirror),
        Err(error) => {
            tracing::error!(%error, "git mirror check failed");
            Err(AccessRefusal::Internal)
        }
    }
}
