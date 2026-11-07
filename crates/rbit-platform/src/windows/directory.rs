use super::{
    PublishedFile, StagedFile,
    binding::{self, Lineage, Root},
    native,
};
use crate::{Component, PlatformError};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Actual local-NTFS owner, NOT a durable-base/prior-head/publication capability.
/// Critical ancestor handles exclude rename; descendants share the real root lease.
#[derive(Clone)]
pub struct OwnedDirectory {
    pub(super) lineage: Arc<Lineage>,
    // Last: namespace exclusion outlives directory-handle destruction.
    pub(super) root: Arc<Root>,
}
impl std::fmt::Debug for OwnedDirectory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OwnedDirectory(..)")
    }
}
impl OwnedDirectory {
    /// Existing controlled directory only. Fresh namespace bootstrap must use
    /// `create_child` from an established durable host base; open/marker is no proof.
    /// Requires cooperating writers and static topology outside held lineage.
    pub fn open_local_ntfs(path: &Path) -> Result<Self, PlatformError> {
        let (root, lineage) = binding::root(path)?;
        let owned = Self { lineage, root };
        owned.check()?;
        Ok(owned)
    }
    pub fn path(&self) -> &Path {
        &self.lineage.path
    }

    pub fn check(&self) -> Result<(), PlatformError> {
        let mut lineage = Some(&self.lineage);
        while let Some(current) = lineage {
            let (_, actual) = binding::directory(&current.path)?;
            if !binding::same_file(actual, current.identity) || !binding::same_file(native::information(&current.file)?, current.identity) {
                return Err(PlatformError::Changed);
            }
            lineage = current.parent.as_ref();
        }
        let held = native::information(&self.root.subject.directory)?;
        if !binding::same_file(held, self.root.identity) || self.root.path != self.top().path {
            return Err(PlatformError::Changed);
        }
        Ok(())
    }
    fn top(&self) -> &Lineage {
        let mut current = self.lineage.as_ref();
        while let Some(parent) = &current.parent {
            current = parent;
        }
        current
    }
    pub(super) fn child_path(&self, name: &Component) -> PathBuf {
        self.path().join(name.as_str())
    }

    /// Existing bindings are validated, not retrospectively called durable.
    pub fn open_child(&self, name: &Component) -> Result<Self, PlatformError> {
        self.check()?;
        if self.lineage.depth >= 16 {
            return Err(PlatformError::InvalidName);
        }
        let path = self.child_path(name);
        let (file, identity) = binding::directory(&path)?;
        if identity.volume != self.root.identity.volume {
            return Err(PlatformError::Unsupported);
        }
        Ok(Self {
            lineage: Arc::new(Lineage {
                path,
                file: Arc::new(file),
                identity,
                parent: Some(Arc::clone(&self.lineage)),
                depth: self.lineage.depth + 1,
            }),
            root: Arc::clone(&self.root),
        })
    }

    /// Publish one NEW empty directory binding with write-through movement.
    pub fn create_child(&self, name: &Component) -> Result<Self, PlatformError> {
        self.check()?;
        if self.lineage.depth >= 16 {
            return Err(PlatformError::InvalidName);
        }
        let target = self.child_path(name);
        if target.try_exists()? {
            return Err(PlatformError::Conflict);
        }
        let temporary = self.child_path(&binding::nonce()?);
        fs::create_dir(&temporary).map_err(binding::conflict)?;
        let (handle, expected) = binding::directory(&temporary)?;
        // Close OUR staging handle before movement; parent lineage stays held.
        drop(handle);
        let mut moved = false;
        let result = (|| {
            native::publish_new(&temporary, &target)?;
            moved = true;
            let created = self.open_child(name)?;
            if !binding::same_file(created.lineage.identity, expected) {
                return Err(PlatformError::Changed);
            }
            Ok(created)
        })();
        // Unpublished nonce only; never recursively remove a published target.
        if !moved && let Ok((handle, actual)) = binding::directory(&temporary) {
            drop(handle);
            if binding::same_file(actual, expected) {
                let _cleanup = fs::remove_dir(&temporary);
            }
        }
        result
    }

    pub fn stage_file(&self) -> Result<StagedFile, PlatformError> {
        StagedFile::new(self.clone())
    }

    /// Git must perform its expected-ref check/lock under the real writer scope.
    /// This primitive merely durably moves a current binding OUT of the live name.
    pub fn retire_ref(&self, name: &Component, trash: &Self) -> Result<PublishedFile, PlatformError> {
        self.check()?;
        trash.check()?;
        if !Arc::ptr_eq(&self.root, &trash.root) {
            return Err(PlatformError::Unsupported);
        }
        let source = self.child_path(name);
        let expected = PublishedFile::inspect(&source)?;
        let retired = binding::nonce()?;
        let target = trash.child_path(&retired);
        if source == target || target.try_exists()? {
            return Err(PlatformError::Conflict);
        }
        native::publish_new(&source, &target)?;
        PublishedFile::after_move(trash.clone(), retired, expected)
    }
}
