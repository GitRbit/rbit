//! Held-file observations, not a hostile-topology or local-volume classifier.
use super::WikiCatalogLeaseError;
use std::{
    fs::{self, File, OpenOptions},
    path::Path,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Identity {
    first: u64,
    second: u64,
}

pub(super) fn no_links(path: &Path) -> Result<(), WikiCatalogLeaseError> {
    for component in path.ancestors() {
        let metadata = match fs::symlink_metadata(component) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            return Err(WikiCatalogLeaseError::Aliased);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(WikiCatalogLeaseError::Aliased);
            }
        }
    }
    Ok(())
}

pub(super) fn open(path: &Path, create: bool) -> Result<File, WikiCatalogLeaseError> {
    no_links(path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(create).create(create).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Keep the subject and lease names from being deleted/renamed while held.
        // This is not a filesystem/volume durability proof.
        options.share_mode(0x1 | 0x2);
    }
    let file = options.open(path)?;
    no_links(path)?;
    identity(&file)?;
    #[cfg(target_os = "linux")]
    reject_file_mount(path, &file)?;
    Ok(file)
}

#[cfg(target_os = "linux")]
fn reject_file_mount(path: &Path, file: &File) -> Result<(), WikiCatalogLeaseError> {
    use rustix::fs::{AtFlags, StatxFlags, statx};
    fn mount(file: &File) -> Result<u64, WikiCatalogLeaseError> {
        let info = statx(file, "", AtFlags::EMPTY_PATH, StatxFlags::MNT_ID).map_err(|error| WikiCatalogLeaseError::Io(error.into()))?;
        if info.stx_mask & StatxFlags::MNT_ID.bits() == 0 || info.stx_mnt_id == 0 {
            return Err(WikiCatalogLeaseError::UnsupportedPlatform);
        }
        Ok(info.stx_mnt_id)
    }
    let parent = File::open(path.parent().ok_or(WikiCatalogLeaseError::NotPersistent)?)?;
    if mount(file)? != mount(&parent)? {
        return Err(WikiCatalogLeaseError::Aliased);
    }
    Ok(())
}

pub(super) fn identity(file: &File) -> Result<Identity, WikiCatalogLeaseError> {
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(WikiCatalogLeaseError::NotPersistent);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(WikiCatalogLeaseError::Aliased);
        }
        Ok(Identity { first: metadata.dev(), second: metadata.ino() })
    }
    #[cfg(windows)]
    {
        let info = winapi_util::file::information(file)?;
        if info.number_of_links() != 1 {
            return Err(WikiCatalogLeaseError::Aliased);
        }
        // Observation only: these IDs are not a uniqueness proof on unknown/ReFS
        // volumes. Positive publication separately requires supported-volume proof.
        Ok(Identity { first: info.volume_serial_number(), second: info.file_index() })
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(WikiCatalogLeaseError::UnsupportedPlatform)
    }
}
