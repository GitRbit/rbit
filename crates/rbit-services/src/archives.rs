//! Source archives (`zip`, `tar.gz`): the adapter over [`rbit_repositories::archives`].
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! The archive application admits, names and starts an archive; this service supplies
//! the instance-wide and per-address slots (`[repository] max_archives_per_ip` for one
//! client), the opened repository's revision lookup and `git archive`. The slot is held
//! by the returned stream and freed when it is dropped. The web layer bounds how long a
//! download may stall or last, so a slow reader cannot keep a slot.

use std::{
    io,
    net::IpAddr,
    pin::Pin,
    task::{Context, Poll},
};

use rbit_git::archive::{ArchiveFormat, ArchiveStream, Archiver};
use rbit_repositories::archives::{ArchivePort, ArchiveRefusal, RepositoryArchives};
pub use rbit_repositories::archives::{MAX_CONCURRENT_ARCHIVES, archive_base_name};
use tokio::io::{AsyncRead, ReadBuf};

use crate::{
    RepoContext, ServiceContext, ServiceError,
    limits::{self, ConcurrencyLimiter, ConcurrencySlot, Refused},
};

/// What a busy instance suggests as `Retry-After`, in seconds.
pub const BUSY_RETRY_SECONDS: u64 = limits::RETRY_AFTER_SECONDS;

/// Why an archive could not be started: [`ArchiveError::Busy`] and
/// [`ArchiveError::TooManyFromClient`] are retried after [`BUSY_RETRY_SECONDS`].
pub type ArchiveError = rbit_repositories::archives::ArchiveError<ServiceError>;

#[derive(Debug, Clone)]
pub struct ArchiveService {
    archiver: Archiver,
    slots: ConcurrencyLimiter,
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

/// One archive request on a repository opened for its viewer.
struct ArchiveRequest<'a> {
    service: &'a ArchiveService,
    repo: &'a RepoContext,
}

impl ArchivePort for ArchiveRequest<'_> {
    type Format = ArchiveFormat;
    type Slot = ConcurrencySlot;
    type Stream = ArchiveStream;
    type Error = ServiceError;

    fn admit(&self, client: IpAddr) -> Result<ConcurrencySlot, ArchiveRefusal> {
        self.service.slots.try_acquire(Some(client)).map_err(|refused| match refused {
            Refused::Busy => ArchiveRefusal::Busy,
            Refused::TooManyFromClient => ArchiveRefusal::TooManyFromClient,
        })
    }

    async fn commit_id(&self, revision: &str) -> Result<String, ServiceError> {
        Ok(self.repo.reader.commit_id(revision.to_owned()).await?)
    }

    async fn start(&self, commit_id: &str, base_name: &str, format: ArchiveFormat) -> Result<ArchiveStream, ServiceError> {
        Ok(self.service.archiver.start(&self.repo.disk_path, commit_id, base_name, format).await?)
    }

    fn extension(format: ArchiveFormat) -> &'static str {
        format.extension()
    }

    fn started(&self, format: ArchiveFormat) {
        metrics::counter!("rbit_archives_total", "format" => format.extension()).increment(1);
    }
}

impl ArchiveService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        let ServiceContext { config, .. } = ctx;
        Self {
            archiver: Archiver::new(config.repository.git_binary.clone(), config.storage.tmp_dir()),
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
        let request = ArchiveRequest { service: self, repo };
        let started = RepositoryArchives::start(&request, repo.repo.repo.name.as_str(), revision, format, client).await?;
        Ok(Archive {
            commit_id: started.commit_id,
            file_name: started.file_name,
            format,
            body: ArchiveBody { stream: started.stream, _slot: started.slot },
        })
    }
}
