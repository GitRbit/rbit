//! Source archives (`zip`, `tar.gz`) of a commit, with bounded concurrency.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! An archive is produced by `git archive` while it is downloaded, so it costs a process
//! and CPU for as long as the client reads. [`ArchiveService`] therefore allows at most
//! [`MAX_CONCURRENT_ARCHIVES`] at once, and `[repository] max_archives_per_ip` of them for
//! one client; the slot is held by the returned stream and freed when it is dropped. When
//! no slot is free the caller is told to retry ([`ArchiveError::Busy`],
//! [`ArchiveError::TooManyFromClient`]) instead of queueing unboundedly. The web layer
//! bounds how long a download may stall or last, so a slow reader cannot keep a slot.

use std::{
    io,
    net::IpAddr,
    pin::Pin,
    task::{Context, Poll},
};

use rbit_git::archive::{ArchiveFormat, ArchiveStream, Archiver};
use tokio::io::{AsyncRead, ReadBuf};

use crate::{
    RepoContext, ServiceContext, ServiceError,
    limits::{self, ConcurrencyLimiter, ConcurrencySlot, Refused},
};

/// Archives produced at the same time, instance-wide.
pub const MAX_CONCURRENT_ARCHIVES: usize = 4;
/// What a busy instance suggests as `Retry-After`, in seconds.
pub const BUSY_RETRY_SECONDS: u64 = limits::RETRY_AFTER_SECONDS;

#[derive(Debug, Clone)]
pub struct ArchiveService {
    archiver: Archiver,
    slots: ConcurrencyLimiter,
}

/// Why an archive could not be started.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    /// Every archive slot is in use; retry after [`BUSY_RETRY_SECONDS`].
    #[error("too many archives are being prepared; try again shortly")]
    Busy,
    /// The client already downloads as many archives as one address may; retry after
    /// [`BUSY_RETRY_SECONDS`].
    #[error("too many archives are being downloaded from this address; try again shortly")]
    TooManyFromClient,
    #[error(transparent)]
    Service(#[from] ServiceError),
}

impl From<Refused> for ArchiveError {
    fn from(refused: Refused) -> Self {
        match refused {
            Refused::Busy => Self::Busy,
            Refused::TooManyFromClient => Self::TooManyFromClient,
        }
    }
}

/// An archive ready to stream.
pub struct Archive {
    /// The commit archived (also a perfect `ETag`: its content never changes).
    pub commit_id: String,
    /// A safe download name, e.g. `rbit-v1.0.tar.gz`.
    pub file_name: String,
    pub format: ArchiveFormat,
    /// The archive's bytes; holds the concurrency slot until dropped.
    pub body: ArchiveBody,
}

impl std::fmt::Debug for Archive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Archive").field("commit_id", &self.commit_id).field("file_name", &self.file_name).finish_non_exhaustive()
    }
}

/// The streamed bytes of an [`Archive`].
pub struct ArchiveBody {
    stream: ArchiveStream,
    _slot: ConcurrencySlot,
}

impl std::fmt::Debug for ArchiveBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArchiveBody").finish_non_exhaustive()
    }
}

impl AsyncRead for ArchiveBody {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        self.stream.as_mut().poll_read(cx, buf)
    }
}

impl ArchiveService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        let ServiceContext { config, .. } = ctx;
        Self {
            archiver: Archiver::new(config.repository.git_binary.clone()),
            slots: ConcurrencyLimiter::new(MAX_CONCURRENT_ARCHIVES, config.repository.max_archives_per_ip()),
        }
    }

    /// Starts an archive of `revision` (a branch, tag or commit id) of a repository the
    /// caller already opened for its viewer, so access is exactly that of browsing.
    /// `client` is the requester's address, for the per-address limit.
    pub async fn archive(
        &self,
        repo: &RepoContext,
        revision: &str,
        format: ArchiveFormat,
        client: IpAddr,
    ) -> Result<Archive, ArchiveError> {
        let slot = self.slots.try_acquire(Some(client))?;
        let commit_id = repo.reader.commit_id(revision.to_owned()).await.map_err(ServiceError::from)?;
        let base_name = archive_base_name(repo.repo.repo.name.as_str(), revision);
        let stream = self.archiver.start(&repo.disk_path, &commit_id, &base_name, format).map_err(ServiceError::from)?;
        metrics::counter!("rbit_archives_total", "format" => format.extension()).increment(1);
        Ok(Archive {
            commit_id,
            file_name: format!("{base_name}{}", format.extension()),
            format,
            body: ArchiveBody { stream, _slot: slot },
        })
    }
}

/// `{repo}-{revision}` with every character outside `[A-Za-z0-9._-]` replaced by `-`, so
/// it is safe as a file name, a `Content-Disposition` value and an archive prefix.
pub fn archive_base_name(repo: &str, revision: &str) -> String {
    let sanitized: String =
        revision.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' }).collect();
    let revision = sanitized.trim_matches(|c| c == '.' || c == '-');
    format!("{repo}-{}", if revision.is_empty() { "archive" } else { revision })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_names_are_safe() {
        assert_eq!(archive_base_name("demo", "v1.0"), "demo-v1.0");
        assert_eq!(archive_base_name("demo", "feature/login"), "demo-feature-login");
        assert_eq!(archive_base_name("demo", "../\"x\"\r\n"), "demo-x");
        assert_eq!(archive_base_name("demo", "..."), "demo-archive");
    }
}
