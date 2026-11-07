use super::{OwnedDirectory, PublishedFile, binding, native};
use crate::{Component, PlatformError};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::windows::fs::OpenOptionsExt,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_WRITE_THROUGH, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE,
};

/// Private create-new staging file. Consume it to publish; dropping only removes
/// its own unpublished nonce, never a published name or recursive directory tree.
pub struct StagedFile {
    file: Option<File>,
    name: Component,
    identity: native::Identity,
    cleanup: bool,
    parent: OwnedDirectory,
}
impl std::fmt::Debug for StagedFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StagedFile(..)")
    }
}
impl StagedFile {
    pub(super) fn new(parent: OwnedDirectory) -> Result<Self, PlatformError> {
        parent.check()?;
        for _ in 0..8 {
            let name = binding::nonce()?;
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                // Synchronous WT file; sharing permits OUR move, though publish
                // also closes the handle after observed sync and before rename.
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_WRITE_THROUGH | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(parent.child_path(&name))
            {
                Ok(file) => {
                    let identity = native::information(&file)?;
                    return Ok(Self { file: Some(file), name, identity, cleanup: true, parent });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(PlatformError::Conflict)
    }
    pub fn publish_new(self, name: &Component) -> Result<PublishedFile, PlatformError> {
        self.publish(name, false)
    }
    /// Mutable ref/config slots only. Git's caller owns expected-value/CAS policy;
    /// this method must never be used to replace an immutable content-addressed object.
    pub fn replace_mutable(self, name: &Component) -> Result<PublishedFile, PlatformError> {
        self.publish(name, true)
    }
    fn publish(mut self, name: &Component, replace: bool) -> Result<PublishedFile, PlatformError> {
        self.parent.check()?;
        if &self.name == name {
            return Err(PlatformError::Conflict);
        }
        let file = self.file.take().ok_or(PlatformError::Changed)?;
        file.sync_all()?;
        let identity = native::information(&file)?;
        if identity.attributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0 || identity.links != 1 {
            return Err(PlatformError::Unsupported);
        }
        drop(file);
        let source = self.parent.child_path(&self.name);
        let target = self.parent.child_path(name);
        if replace {
            if target.try_exists()? {
                PublishedFile::inspect(&target)?;
            }
            native::replace_mutable(&source, &target)?;
        } else {
            if target.try_exists()? {
                return Err(PlatformError::Conflict);
            }
            native::publish_new(&source, &target)?;
        }
        // A successful move frees this nonce. Never delete a later file that
        // happens to acquire the same nonce after our completed movement.
        self.cleanup = false;
        PublishedFile::after_move(self.parent.clone(), name.clone(), identity)
    }
}
impl Write for StagedFile {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.file.as_mut().ok_or_else(|| io::Error::other("staging file already consumed"))?.write(buffer)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.as_mut().ok_or_else(|| io::Error::other("staging file already consumed"))?.flush()
    }
}
impl Drop for StagedFile {
    fn drop(&mut self) {
        drop(self.file.take());
        let path = self.parent.child_path(&self.name);
        if self.cleanup && PublishedFile::inspect(&path).is_ok_and(|actual| binding::same_file(actual, self.identity)) {
            let _cleanup = fs::remove_file(path);
        }
    }
}
