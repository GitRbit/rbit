//! Host-owned staging of validated archives as immutable content-addressed objects.
//!
//! The root must already exist durably and remain private and stable for the
//! store's entire lifetime. On Windows its ACL must be provisioned by the host;
//! portable std APIs do not verify ACLs. This module does not defend against an
//! actor able to replace root ancestors or mutate host-owned staging paths.
//!
//! No payload paths are extracted, permissions applied, trust granted, ledger
//! records written, live intent changed, or code executed. A verified existing
//! Windows object has unconfirmed namespace durability: installation must consult
//! a prior durable ledger receipt or explicitly reconcile an unreferenced object.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use rbit_plugin_api::{PackageManifest, Sha256Digest};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::{ArchiveError, MAX_ARCHIVE_WIRE_BYTES, ValidatedArchive, validate_archive};

const IO_BYTES: usize = 64 * 1024;

/// Cooperative bounds. Callers retain their owned admission lease until the
/// operation returns, including publication, synchronization and cleanup.
/// Blocking Read/fsync calls require host transport/operation deadlines as well.
#[derive(Debug, Clone)]
pub struct StageControl {
    deadline: Instant,
    max_wire_bytes: u64,
    cancelled: Arc<AtomicBool>,
}
impl StageControl {
    /// Reserve room for the validator's final one-byte EOF probe in the limit.
    pub fn new(deadline: Instant, max_wire_bytes: u64) -> Result<Self, StoreError> {
        if max_wire_bytes == 0 || max_wire_bytes > MAX_ARCHIVE_WIRE_BYTES {
            return Err(StoreError::Control);
        }
        Ok(Self { deadline, max_wire_bytes, cancelled: Arc::new(AtomicBool::new(false)) })
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    fn check(&self) -> Result<(), StoreError> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(StoreError::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(StoreError::Deadline);
        }
        Ok(())
    }
}

/// Whether this call created the object or verified an already present object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationKind {
    Created,
    ExistingVerified,
}
/// Whether an error occurred before publication was attempted or after it may
/// have changed the namespace. Verified means a receipt describes matching bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationVisibility {
    NotAttempted,
    Unconfirmed,
    Verified,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    Confirmed,
    Unconfirmed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupStatus {
    Complete,
    Pending,
}

/// Read-only object identity and observed publication state, not install authority.
#[derive(Debug, Clone)]
pub struct PackageReceipt {
    digest: Sha256Digest,
    archive: Arc<ValidatedArchive>,
    publication: PublicationKind,
    durability: Durability,
}
impl PackageReceipt {
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }
    pub fn manifest(&self) -> &PackageManifest {
        self.archive.manifest()
    }
    pub fn wire_bytes(&self) -> u64 {
        self.archive.wire_bytes()
    }
    pub fn payload_bytes(&self) -> u64 {
        self.archive.payload_bytes()
    }
    pub const fn publication(&self) -> PublicationKind {
        self.publication
    }
    pub const fn durability(&self) -> Durability {
        self.durability
    }
}

/// Static errors do not expose arbitrary source, filesystem or archive contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("package store root is unavailable or is not a nonsymlink directory")]
    Root,
    #[error("package store root is not private")]
    PrivateRoot,
    #[error("invalid package staging control bounds")]
    Control,
    #[error("package operation was cancelled")]
    Cancelled,
    #[error("package operation deadline expired")]
    Deadline,
    #[error("package staging exceeded its wire quota")]
    WireLimit,
    #[error("private package staging could not be created")]
    Staging,
    #[error("package source read failed")]
    Read,
    #[error("private package staging write failed")]
    Write,
    #[error("package archive validation failed: {0}")]
    Archive(ArchiveError),
    #[error("package file synchronization failed")]
    Sync,
    #[error("package object publication failed")]
    Publish,
    #[error("existing package object is not a matching nonsymlink regular file")]
    CorruptExisting,
    #[error("package directory synchronization failed")]
    DirectorySync,
    #[error("package staging cleanup is incomplete")]
    Cleanup,
    #[error("package publication is unsupported on this platform")]
    UnsupportedPlatform,
}

/// An error can follow publication. Inspect the receipt rather than assuming an
/// error means the target is absent. Published objects are never rolled back.
#[derive(Debug, thiserror::Error)]
#[error("{error}")]
pub struct StoreFailure {
    #[source]
    error: StoreError,
    receipt: Option<PackageReceipt>,
    cleanup: CleanupStatus,
    visibility: PublicationVisibility,
}
impl StoreFailure {
    pub const fn error(&self) -> StoreError {
        self.error
    }
    pub fn receipt(&self) -> Option<&PackageReceipt> {
        self.receipt.as_ref()
    }
    pub const fn cleanup(&self) -> CleanupStatus {
        self.cleanup
    }
    pub const fn visibility(&self) -> PublicationVisibility {
        self.visibility
    }
}

/// The host must provision this root durably, with exclusive write authority.
/// Only digest-derived ASCII object names are used; plugin IDs and versions are
/// metadata, so Windows reserved names and path encodings cannot affect storage.
#[derive(Debug, Clone)]
pub struct PackageStore {
    root: Arc<PathBuf>,
}
impl PackageStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let root = root.as_ref();
        let metadata = fs::symlink_metadata(root).map_err(|_| StoreError::Root)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(StoreError::Root);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(StoreError::PrivateRoot);
            }
        }
        let root = fs::canonicalize(root).map_err(|_| StoreError::Root)?;
        Ok(Self { root: Arc::new(root) })
    }

    /// Tee verified bytes into one private inode. There is no validate-then-reopen
    /// copy, no payload-sized allocation, and no path supplied by package contents.
    pub fn stage(&self, reader: impl Read, control: StageControl) -> Result<PreparedPackage, StoreFailure> {
        control.check().map_err(|error| failure(error, None, None))?;
        let mut builder = tempfile::Builder::new();
        builder.prefix(".rbit-stage-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(fs::Permissions::from_mode(0o700));
        }
        let directory = builder.tempdir_in(self.root.as_ref()).map_err(|_| failure(StoreError::Staging, None, None))?;
        let source = directory.path().join("archive.rbit-plugin");
        let result = (|| {
            control.check()?;
            let mut options = OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            // A normal file deliberately avoids Windows FILE_ATTRIBUTE_TEMPORARY.
            let mut output = options.open(&source).map_err(|_| StoreError::Staging)?;
            let mut tee = StagingReader { reader, output: &mut output, control: &control, digest: Sha256::new(), bytes: 0, error: None };
            let validated = validate_archive(&mut tee);
            if let Some(error) = tee.error {
                return Err(error);
            }
            let archive = validated.map_err(StoreError::Archive)?;
            let digest = encode_digest(&tee.digest.finalize());
            control.check()?;
            output.flush().map_err(|_| StoreError::Write)?;
            control.check()?;
            output.sync_all().map_err(|_| StoreError::Sync)?;
            control.check()?;
            Ok((archive, digest))
        })();
        match result {
            Ok((archive, digest)) => Ok(PreparedPackage {
                root: Arc::clone(&self.root),
                directory: Some(directory),
                source,
                archive: Arc::new(archive),
                digest,
                control,
            }),
            Err(error) => Err(failure(error, Some(directory), None)),
        }
    }
}

/// Owns a private synchronized archive. No writable handle or staging path is
/// exposed. Use discard to observe cleanup failure; dropping is best-effort only.
#[derive(Debug)]
pub struct PreparedPackage {
    root: Arc<PathBuf>,
    directory: Option<TempDir>,
    source: PathBuf,
    archive: Arc<ValidatedArchive>,
    digest: Sha256Digest,
    control: StageControl,
}
impl PreparedPackage {
    pub fn manifest(&self) -> &PackageManifest {
        self.archive.manifest()
    }
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }

    pub fn discard(mut self) -> Result<(), StoreFailure> {
        let cleanup = cleanup(self.directory.take());
        if cleanup == CleanupStatus::Complete {
            Ok(())
        } else {
            Err(StoreFailure { error: StoreError::Cleanup, receipt: None, cleanup, visibility: PublicationVisibility::NotAttempted })
        }
    }

    /// Publishes without replacement. Cancellation is checked before publication;
    /// after the target becomes visible, synchronization and cleanup still run.
    pub fn publish(mut self) -> Result<PackageReceipt, StoreFailure> {
        self.control.check().map_err(|error| failure(error, self.directory.take(), None))?;
        let target = self.root.join(format!("{}.rbit-plugin", self.digest.as_str()));
        let publication = match publish_object(&self.source, &target) {
            Ok(()) => PublicationKind::Created,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if let Err(error) = verify_existing(&target, &self.archive, &self.digest, &self.control) {
                    let mut failure = failure(error, self.directory.take(), None);
                    failure.visibility = PublicationVisibility::Unconfirmed;
                    return Err(failure);
                }
                PublicationKind::ExistingVerified
            }
            Err(_) => {
                // An OS error, especially during write-through, must not imply
                // rollback. Observe a matching target conservatively when possible.
                let verification = verify_existing(&target, &self.archive, &self.digest, &self.control);
                let error = match verification {
                    Err(StoreError::Cancelled) => StoreError::Cancelled,
                    Err(StoreError::Deadline) => StoreError::Deadline,
                    Err(StoreError::Sync) => StoreError::Sync,
                    Err(_) if fs::symlink_metadata(&target).is_ok() => StoreError::CorruptExisting,
                    _ => StoreError::Publish,
                };
                let receipt = verification.ok().map(|()| PackageReceipt {
                    digest: self.digest.clone(),
                    archive: Arc::clone(&self.archive),
                    publication: PublicationKind::ExistingVerified,
                    durability: Durability::Unconfirmed,
                });
                let mut failure = failure(error, self.directory.take(), receipt);
                if failure.receipt.is_none() {
                    failure.visibility = PublicationVisibility::Unconfirmed;
                }
                return Err(failure);
            }
        };
        let mut receipt = PackageReceipt {
            digest: self.digest.clone(),
            archive: Arc::clone(&self.archive),
            publication,
            durability: Durability::Unconfirmed,
        };
        match confirm_publication(self.root.as_ref(), publication) {
            Ok(durability) => receipt.durability = durability,
            Err(error) => return Err(failure(error, self.directory.take(), Some(receipt))),
        }
        let status = cleanup(self.directory.take());
        if status == CleanupStatus::Pending {
            return Err(StoreFailure {
                error: StoreError::Cleanup,
                receipt: Some(receipt),
                cleanup: status,
                visibility: PublicationVisibility::Verified,
            });
        }
        Ok(receipt)
    }
}

fn failure(error: StoreError, directory: Option<TempDir>, receipt: Option<PackageReceipt>) -> StoreFailure {
    let visibility = if receipt.is_some() { PublicationVisibility::Verified } else { PublicationVisibility::NotAttempted };
    StoreFailure { error, receipt, cleanup: cleanup(directory), visibility }
}
fn cleanup(directory: Option<TempDir>) -> CleanupStatus {
    if directory.is_none_or(|directory| directory.close().is_ok()) { CleanupStatus::Complete } else { CleanupStatus::Pending }
}

struct StagingReader<'a, R> {
    reader: R,
    output: &'a mut File,
    control: &'a StageControl,
    digest: Sha256,
    bytes: u64,
    error: Option<StoreError>,
}
impl<R: Read> Read for StagingReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let result = (|| {
            self.control.check()?;
            let available = self.control.max_wire_bytes.checked_sub(self.bytes).ok_or(StoreError::WireLimit)?;
            let count = usize::try_from(available.min(buffer.len() as u64).min(IO_BYTES as u64)).map_err(|_| StoreError::WireLimit)?;
            if count == 0 {
                return Err(StoreError::WireLimit);
            }
            let read = match self.reader.read(&mut buffer[..count]) {
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => return Ok(None),
                Err(_) => return Err(StoreError::Read),
            };
            self.control.check()?;
            self.bytes = self.bytes.checked_add(read as u64).ok_or(StoreError::WireLimit)?;
            self.output.write_all(&buffer[..read]).map_err(|_| StoreError::Write)?;
            self.control.check()?;
            self.digest.update(&buffer[..read]);
            Ok(Some(read))
        })();
        match result {
            Ok(Some(read)) => Ok(read),
            Ok(None) => Err(io::Error::from(io::ErrorKind::Interrupted)),
            Err(error) => {
                self.error = Some(error);
                Err(io::Error::other("package staging stopped"))
            }
        }
    }
}

fn encode_digest(bytes: &[u8]) -> Sha256Digest {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text.parse().expect("SHA256 output has canonical digest syntax")
}
fn verify_existing(path: &Path, archive: &ValidatedArchive, digest: &Sha256Digest, control: &StageControl) -> Result<(), StoreError> {
    control.check()?;
    let metadata = fs::symlink_metadata(path).map_err(|_| StoreError::CorruptExisting)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != archive.wire_bytes() {
        return Err(StoreError::CorruptExisting);
    }
    let mut file = File::open(path).map_err(|_| StoreError::CorruptExisting)?;
    let opened = file.metadata().map_err(|_| StoreError::CorruptExisting)?;
    if !opened.is_file() || opened.len() != archive.wire_bytes() {
        return Err(StoreError::CorruptExisting);
    }
    let mut hasher = Sha256::new();
    let mut remaining = archive.wire_bytes();
    let mut buffer = vec![0; IO_BYTES];
    while remaining != 0 {
        control.check()?;
        let count = usize::try_from(remaining.min(IO_BYTES as u64)).map_err(|_| StoreError::CorruptExisting)?;
        let read = match file.read(&mut buffer[..count]) {
            Ok(0) => return Err(StoreError::CorruptExisting),
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(StoreError::CorruptExisting),
        };
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    control.check()?;
    loop {
        control.check()?;
        match file.read(&mut [0; 1]) {
            Ok(0) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            _ => return Err(StoreError::CorruptExisting),
        }
    }
    control.check()?;
    if &encode_digest(&hasher.finalize()) != digest {
        return Err(StoreError::CorruptExisting);
    }
    // A matching preexisting object may be unreferenced or still dirty. Unix
    // directory fsync alone cannot certify its content durability.
    #[cfg(unix)]
    file.sync_all().map_err(|_| StoreError::Sync)?;
    control.check()?;
    Ok(())
}

#[cfg(unix)]
fn publish_object(source: &Path, target: &Path) -> io::Result<()> {
    fs::hard_link(source, target)
}
#[cfg(windows)]
fn publish_object(source: &Path, target: &Path) -> io::Result<()> {
    atomicwrites::move_atomic(source, target)
}
#[cfg(not(any(unix, windows)))]
fn publish_object(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

#[cfg(unix)]
fn confirm_publication(root: &Path, _: PublicationKind) -> Result<Durability, StoreError> {
    File::open(root).and_then(|directory| directory.sync_all()).map_err(|_| StoreError::DirectorySync)?;
    Ok(Durability::Confirmed)
}
#[cfg(windows)]
#[allow(clippy::unnecessary_wraps)] // Keep the same fallible publication contract on every platform.
fn confirm_publication(_: &Path, publication: PublicationKind) -> Result<Durability, StoreError> {
    Ok(match publication {
        PublicationKind::Created => Durability::Confirmed,
        PublicationKind::ExistingVerified => Durability::Unconfirmed,
    })
}
#[cfg(not(any(unix, windows)))]
fn confirm_publication(_: &Path, _: PublicationKind) -> Result<Durability, StoreError> {
    Err(StoreError::UnsupportedPlatform)
}
