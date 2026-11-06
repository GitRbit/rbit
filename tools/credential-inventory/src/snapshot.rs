//! Offline-file checks are drift detection, not a lock or consistent-live-snapshot proof.
use crate::ScanError;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};
/// Bounds full-file hashing work. This is a tool-input bound, not an account policy.
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
#[derive(PartialEq, Eq)]
pub(crate) struct Fingerprint {
    length: u64,
    modified: SystemTime,
    created: Option<SystemTime>,
    digest: [u8; 32],
}
pub(crate) fn checked_path(path: &Path) -> Result<PathBuf, ScanError> {
    let absolute = std::path::absolute(path).map_err(|_| ScanError::Input)?;
    for ancestor in absolute.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(|_| ScanError::Input)?;
        if linked(&metadata) {
            return Err(ScanError::Input);
        }
    }
    let canonical = absolute.canonicalize().map_err(|_| ScanError::Input)?;
    if !fs::metadata(&canonical).map_err(|_| ScanError::Input)?.is_file() {
        return Err(ScanError::Input);
    }
    no_sidecars(&canonical)?;
    Ok(canonical)
}
pub(crate) fn no_sidecars(path: &Path) -> Result<(), ScanError> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        match fs::symlink_metadata(PathBuf::from(name)) {
            Ok(_) => return Err(ScanError::Sidecar),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(ScanError::Input),
        }
    }
    Ok(())
}
pub(crate) fn fingerprint(path: &Path) -> Result<Fingerprint, ScanError> {
    no_sidecars(path)?;
    let mut file = File::open(path).map_err(|_| ScanError::Input)?;
    let before = file.metadata().map_err(|_| ScanError::Input)?;
    if !before.is_file() || before.len() > MAX_FILE_BYTES {
        return Err(ScanError::Input);
    }
    let mut hasher = Sha256::new();
    let mut buffer = zeroize::Zeroizing::new([0; 64 * 1024]);
    let mut length = 0_u64;
    loop {
        let read = file.read(&mut *buffer).map_err(|_| ScanError::Input)?;
        if read == 0 {
            break;
        }
        length = length.checked_add(read as u64).ok_or(ScanError::Input)?;
        if length > MAX_FILE_BYTES {
            return Err(ScanError::Input);
        }
        hasher.update(&buffer[..read]);
    }
    let after = file.metadata().map_err(|_| ScanError::Input)?;
    if length != before.len() || length != after.len() || before.modified().ok() != after.modified().ok() {
        return Err(ScanError::Drift);
    }
    no_sidecars(path)?;
    Ok(Fingerprint {
        length,
        modified: after.modified().map_err(|_| ScanError::Input)?,
        created: after.created().ok(),
        digest: hasher.finalize().into(),
    })
}

fn linked(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

impl std::fmt::Debug for Fingerprint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Fingerprint").finish_non_exhaustive()
    }
}
