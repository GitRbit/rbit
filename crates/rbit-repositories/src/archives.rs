//! Source archives of a commit, with bounded concurrency.
//!
//! An archive is produced while it is downloaded, so it costs a process and CPU for as
//! long as the client reads. At most [`MAX_CONCURRENT_ARCHIVES`] run at once, and a
//! configured number for one client address. Admission comes first and never waits: a
//! refused request does no Git work and is told to retry ([`ArchiveError::Busy`],
//! [`ArchiveError::TooManyFromClient`]). The slot travels with the stream and is freed
//! when the stream is dropped. Access is decided before: the caller opened the
//! repository for its viewer exactly as for browsing.

use std::{fmt, future::Future, net::IpAddr};

/// Archives produced at the same time, instance-wide.
pub const MAX_CONCURRENT_ARCHIVES: usize = 4;

/// Why admission refused an archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveRefusal {
    /// Every archive slot is in use.
    Busy,
    /// The client already downloads as many archives as one address may.
    TooManyFromClient,
}

/// Why an archive could not be started; retry refusals after the caller's delay.
#[derive(Debug)]
pub enum ArchiveError<E> {
    Busy,
    TooManyFromClient,
    /// The adapter's own failure (revision lookup or Git), unchanged.
    Service(E),
}

impl<E> From<ArchiveRefusal> for ArchiveError<E> {
    fn from(refusal: ArchiveRefusal) -> Self {
        match refusal {
            ArchiveRefusal::Busy => Self::Busy,
            ArchiveRefusal::TooManyFromClient => Self::TooManyFromClient,
        }
    }
}

impl<E: fmt::Display> fmt::Display for ArchiveError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => f.write_str("too many archives are being prepared; try again shortly"),
            Self::TooManyFromClient => f.write_str("too many archives are being downloaded from this address; try again shortly"),
            Self::Service(error) => error.fmt(f),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for ArchiveError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Service(error) => error.source(),
            _ => None,
        }
    }
}

/// Admission, revision lookup and the archive process for one opened repository.
pub trait ArchivePort: Sync {
    type Format: Copy + Send + Sync;
    type Slot: Send;
    type Stream: Send;
    type Error: Send;

    fn admit(&self, client: IpAddr) -> Result<Self::Slot, ArchiveRefusal>;
    fn commit_id(&self, revision: &str) -> impl Future<Output = Result<String, Self::Error>> + Send;
    /// Starts the archive of `commit_id` with every entry under `base_name/`.
    fn start(
        &self,
        commit_id: &str,
        base_name: &str,
        format: Self::Format,
    ) -> impl Future<Output = Result<Self::Stream, Self::Error>> + Send;
    /// The file name suffix of `format`, such as `.tar.gz`.
    fn extension(format: Self::Format) -> &'static str;
    fn started(&self, format: Self::Format);
}

/// A started archive; dropping `slot` (with the stream) frees the place.
#[derive(Debug)]
pub struct StartedArchive<S, L> {
    /// The commit archived (also a perfect `ETag`: its content never changes).
    pub commit_id: String,
    /// A safe download name, e.g. `rbit-v1.0.tar.gz`.
    pub file_name: String,
    pub stream: S,
    pub slot: L,
}

/// The archive command.
#[derive(Debug)]
pub struct RepositoryArchives;

impl RepositoryArchives {
    /// Admits, then resolves `revision` (a branch, tag or commit id) and starts the archive.
    pub async fn start<P: ArchivePort>(
        port: &P,
        repo_name: &str,
        revision: &str,
        format: P::Format,
        client: IpAddr,
    ) -> Result<StartedArchive<P::Stream, P::Slot>, ArchiveError<P::Error>> {
        let slot = port.admit(client)?;
        let commit_id = port.commit_id(revision).await.map_err(ArchiveError::Service)?;
        let base_name = archive_base_name(repo_name, revision);
        let stream = port.start(&commit_id, &base_name, format).await.map_err(ArchiveError::Service)?;
        port.started(format);
        Ok(StartedArchive { commit_id, file_name: format!("{base_name}{}", P::extension(format)), stream, slot })
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
