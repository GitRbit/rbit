//! Actual file ownership and private generated staging names; no FFI here.
use super::native::{self, Identity, Subject};
use crate::{Component, PlatformError};
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::Arc,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
    FILE_SHARE_WRITE,
};

pub(super) struct Root {
    pub(super) path: PathBuf,
    pub(super) identity: Identity,
    // Last: hold real subject exclusion after root path/file state destruction.
    pub(super) subject: Subject,
}

pub(super) struct Lineage {
    pub(super) path: PathBuf,
    pub(super) file: Arc<File>,
    pub(super) identity: Identity,
    pub(super) parent: Option<Arc<Lineage>>,
    pub(super) depth: u8,
}

pub(super) fn no_links(path: &Path) -> Result<(), PlatformError> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        if metadata.file_type().is_symlink() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(PlatformError::Unsupported);
        }
    }
    Ok(())
}

pub(super) fn directory(path: &Path) -> Result<(File, Identity), PlatformError> {
    no_links(path)?;
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let identity = native::information(&file)?;
    if identity.attributes & FILE_ATTRIBUTE_DIRECTORY == 0 || identity.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(PlatformError::Unsupported);
    }
    no_links(path)?;
    Ok((file, identity))
}

pub(super) fn root(path: &Path) -> Result<(Arc<Root>, Arc<Lineage>), PlatformError> {
    root_with(path, native::classify)
}

// Private fault seam; production always uses the actual held-handle classifier.
pub(super) fn root_with(
    path: &Path,
    classify: impl FnOnce(Subject) -> Result<Subject, PlatformError>,
) -> Result<(Arc<Root>, Arc<Lineage>), PlatformError> {
    if !path.is_absolute() {
        return Err(PlatformError::InvalidName);
    }
    no_links(path)?;
    let path = path.canonicalize()?;
    let (file, identity) = directory(&path)?;
    let mut filename = path.file_name().ok_or(PlatformError::Unsupported)?.to_os_string();
    filename.push(".platform-writer-v1.lock");
    let lock_path = path.with_file_name(filename);
    if lock_path.try_exists()? {
        no_links(&lock_path)?;
    }
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&lock_path)?;
    let lock_identity = native::information(&lease)?;
    if lease.metadata()?.len() != 0
        || lock_identity.links != 1
        || lock_identity.attributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
    {
        return Err(PlatformError::Unsupported);
    }
    lease.try_lock().map_err(|error| match error {
        TryLockError::WouldBlock => PlatformError::Busy,
        TryLockError::Error(error) => error.into(),
    })?;
    no_links(&lock_path)?;
    // Pending/unknown native completion retains BOTH real handles and heap outputs.
    let subject = classify(Subject { directory: file, _lease: lease })?;
    let file = Arc::new(subject.directory.try_clone()?);
    let lineage = Arc::new(Lineage { path: path.clone(), file, identity, parent: None, depth: 0 });
    Ok((Arc::new(Root { path, identity, subject }), lineage))
}

pub(super) fn same_file(left: Identity, right: Identity) -> bool {
    left.volume == right.volume && left.index == right.index
}

pub(super) fn nonce() -> Result<Component, PlatformError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| PlatformError::Unsupported)?;
    use std::fmt::Write;
    let mut name = String::from(".stage-");
    for byte in bytes {
        write!(name, "{byte:02x}").map_err(|_| PlatformError::Unsupported)?;
    }
    Component::new(&name)
}

pub(super) fn conflict(error: std::io::Error) -> PlatformError {
    if error.kind() == std::io::ErrorKind::AlreadyExists { PlatformError::Conflict } else { error.into() }
}
