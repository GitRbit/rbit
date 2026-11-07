//! Numeric object layout and bounded catalog marker; never a user page path.
use super::WikiObjectError;
use rbit_core::RepoId;
use rbit_wikis::WikiStoreId;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MARKER: &str = "wiki-store-v1";

fn io(error: std::io::Error) -> WikiObjectError {
    crate::GitError::Io(error).into()
}

pub(super) fn no_links(path: &Path) -> Result<(), WikiObjectError> {
    for ancestor in path.ancestors() {
        let metadata = match fs::symlink_metadata(ancestor) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io(error)),
        };
        if metadata.file_type().is_symlink() {
            return Err(WikiObjectError::Binding);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(WikiObjectError::Binding);
            }
        }
    }
    Ok(())
}

pub(super) fn bind_root(root: &Path, identity: WikiStoreId) -> Result<PathBuf, WikiObjectError> {
    if !root.is_absolute() {
        return Err(WikiObjectError::Binding);
    }
    no_links(root)?;
    fs::create_dir_all(root).map_err(io)?;
    no_links(root)?;
    let marker = root.join(MARKER);
    no_links(&marker)?;
    if !marker.try_exists().map_err(io)? && fs::read_dir(root).map_err(io)?.next().transpose().map_err(io)?.is_some() {
        return Err(WikiObjectError::Binding);
    }
    match OpenOptions::new().write(true).create_new(true).open(&marker) {
        Ok(mut file) => {
            file.write_all(identity.as_bytes()).map_err(io)?;
            file.sync_all().map_err(io)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(io(error)),
    }
    verify_root(root, identity)?;
    root.canonicalize().map_err(io)
}

pub(super) fn verify_root(root: &Path, identity: WikiStoreId) -> Result<(), WikiObjectError> {
    let marker = root.join(MARKER);
    no_links(&marker)?;
    let mut file = OpenOptions::new().read(true).open(marker).map_err(io)?;
    let metadata = file.metadata().map_err(io)?;
    if !metadata.is_file() || metadata.len() != 16 {
        return Err(WikiObjectError::Binding);
    }
    let mut bytes = [0; 16];
    file.read_exact(&mut bytes).map_err(io)?;
    let mut extra = [0; 1];
    if file.read(&mut extra).map_err(io)? != 0 || bytes != *identity.as_bytes() {
        return Err(WikiObjectError::Binding);
    }
    Ok(())
}

pub(super) fn repository(root: &Path, repository: RepoId, create: bool) -> Result<gix::Repository, WikiObjectError> {
    if repository.0 <= 0 {
        return Err(WikiObjectError::Binding);
    }
    let path = root.join(format!("{}.git", repository.0));
    no_links(&path)?;
    if create {
        match fs::create_dir(&path) {
            Ok(()) => {
                gix::init_bare(&path).map_err(crate::GitError::backend)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(io(error)),
        }
    }
    for component in ["HEAD", "config", "objects", "objects/pack", "objects/info", "refs"] {
        no_links(&path.join(component))?;
    }
    if path.join("objects/info/alternates").try_exists().map_err(io)? {
        return Err(WikiObjectError::Binding);
    }
    let repo = gix::open_opts(&path, gix::open::Options::isolated()).map_err(crate::GitError::backend)?;
    if repo.work_dir().is_some() {
        return Err(WikiObjectError::Binding);
    }
    Ok(repo)
}
