use super::{OwnedDirectory, binding, native};
use crate::{Component, PlatformError};
use std::{
    fs::OpenOptions,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE,
};

/// Actual successful WT namespace operation retaining its owner. Not Wiki authority.
pub struct PublishedFile {
    name: Component,
    identity: native::Identity,
    parent: OwnedDirectory,
}
impl std::fmt::Debug for PublishedFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PublishedFile(..)")
    }
}
impl PublishedFile {
    pub fn path(&self) -> PathBuf {
        self.parent.child_path(&self.name)
    }
    pub fn check(&self) -> Result<(), PlatformError> {
        self.parent.check()?;
        if !binding::same_file(Self::inspect(&self.path())?, self.identity) {
            return Err(PlatformError::Changed);
        }
        Ok(())
    }
    pub(super) fn inspect(path: &Path) -> Result<native::Identity, PlatformError> {
        binding::no_links(path)?;
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        let info = native::information(&file)?;
        if info.attributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0 || info.links != 1 {
            return Err(PlatformError::Unsupported);
        }
        Ok(info)
    }
    pub(super) fn after_move(parent: OwnedDirectory, name: Component, identity: native::Identity) -> Result<Self, PlatformError> {
        let value = Self { name, identity, parent };
        value.check()?;
        Ok(value)
    }
}
