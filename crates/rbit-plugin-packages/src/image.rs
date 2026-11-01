//! Tentative native payload materialization. No execution or policy authority.
//!
//! Roots/ancestors must be host-owned, private and stable. Same-UID native code is
//! fully trusted OS-user code; these files are not tamper-proof against it. Unix
//! only until an audited Windows ACL/root policy exists. Images are ephemeral
//! derivatives of durable CAS, not durable installation receipts.
use crate::{ArchiveError, CleanupStatus, PackageStore, StageControl};
use rbit_plugin_api::{InstalledPackageRef, PackageManifest};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    #[error("native image materialization is unsupported on this host")]
    Unsupported,
    #[error("native image root is not private, usable and separate from CAS")]
    Root,
    #[error("native image control or expected size is invalid")]
    Control,
    #[error("native image operation was cancelled")]
    Cancelled,
    #[error("native image deadline expired")]
    Deadline,
    #[error("native image wire quota exhausted")]
    Quota,
    #[error("CAS object is not a bounded nonsymlink regular file")]
    Object,
    #[error("CAS object read failed")]
    Read,
    #[error("native image payload write failed")]
    Write,
    #[error("native image declaration or archive identity differs")]
    Identity,
    #[error("package is not a native package")]
    Kind,
    #[error("native image cleanup is incomplete")]
    Cleanup,
    #[error("invalid native image archive: {0}")]
    Archive(ArchiveError),
}

/// Bounded static diagnostics. A retained cleanup path is for the trusted host
/// coordinator only, not an HTTP response or package-controlled destination.
pub struct ImageFailure {
    error: ImageError,
    cleanup: CleanupStatus,
    residual: Option<PathBuf>,
}
impl std::fmt::Debug for ImageFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageFailure").field("error", &self.error).field("cleanup", &self.cleanup).finish_non_exhaustive()
    }
}
impl std::fmt::Display for ImageFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}
impl std::error::Error for ImageFailure {}
impl ImageFailure {
    pub fn error(&self) -> ImageError {
        self.error
    }
    pub fn cleanup(&self) -> CleanupStatus {
        self.cleanup
    }
    pub fn residual_path(&self) -> Option<&Path> {
        self.residual.as_deref()
    }
    fn clean(error: ImageError) -> Self {
        Self { error, cleanup: CleanupStatus::Complete, residual: None }
    }
}

#[derive(Debug, Clone)]
pub struct NativeImageStore {
    root: Arc<PathBuf>,
}
impl NativeImageStore {
    /// Checks Unix type/mode only; host ownership, ACLs, ancestors and stability
    /// remain caller preconditions. No provisioning/marker-file privacy proof.
    /// Rejects unsupported hosts.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, ImageError> {
        #[cfg(unix)]
        {
            platform::open(root.as_ref()).map(|root| Self { root: Arc::new(root) })
        }
        #[cfg(not(unix))]
        {
            let _ = root;
            Err(ImageError::Unsupported)
        }
    }
    /// Synchronous bounded local I/O. Caller retains finite disk/concurrency
    /// admission until this returns AND any explicit cleanup has settled. Dropping
    /// an async waiter must not detach/cancel a still-running blocking operation.
    /// Neither `StageControl` nor byte caps interrupt a blocked filesystem syscall.
    pub fn prepare(
        &self,
        cas: &PackageStore,
        expected: &InstalledPackageRef,
        wire_bytes: u64,
        control: StageControl,
    ) -> Result<PreparedNativeImage, ImageFailure> {
        #[cfg(unix)]
        {
            platform::prepare(self, cas, expected, wire_bytes, control)
        }
        #[cfg(not(unix))]
        {
            let _ = (&self.root, cas, expected, wire_bytes, control);
            Err(ImageFailure::clean(ImageError::Unsupported))
        }
    }
}

/// Complete verified payload bytes, still non-executable and NOT a launch permit.
/// Fields are private; no writable handle or clone escapes. Files remain `0600` and
/// directories `0700`. Paths are exposed only to trusted host composition code.
#[derive(Debug)]
pub struct PreparedNativeImage {
    directory: tempfile::TempDir,
    expected: InstalledPackageRef,
    manifest: PackageManifest,
    entrypoint: PathBuf,
    wire_bytes: u64,
}
impl PreparedNativeImage {
    pub fn package(&self) -> &InstalledPackageRef {
        &self.expected
    }
    pub fn manifest(&self) -> &PackageManifest {
        &self.manifest
    }
    pub fn entrypoint(&self) -> &Path {
        &self.entrypoint
    }
    pub fn root(&self) -> &Path {
        self.directory.path()
    }
    pub fn wire_bytes(&self) -> u64 {
        self.wire_bytes
    }
    /// Explicit cleanup reports failures. Dropping `TempDir` is synchronous best
    /// effort only; a coordinator must use this method to account for failure.
    pub fn discard(self) -> Result<(), ImageFailure> {
        let path = self.directory.path().to_path_buf();
        self.directory.close().map_err(|_| ImageFailure {
            error: ImageError::Cleanup,
            cleanup: CleanupStatus::Pending,
            residual: Some(path),
        })
    }
}
#[cfg(unix)]
mod platform;
