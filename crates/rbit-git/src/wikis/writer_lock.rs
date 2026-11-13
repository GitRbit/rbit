//! One held cooperating-writer lock, not filesystem integrity or durability proof.
use super::{WikiObjectError, layout};
use rbit_kernel::AdmissionError;
use std::{
    fs::{File, OpenOptions, TryLockError},
    path::Path,
};

/// Unlock explicitly when the actual owner finishes. Closing our descriptor alone
/// need not release a Unix flock: another thread can fork a child that briefly
/// inherits the same open file description before exec closes its CLOEXEC handle.
#[derive(Debug)]
pub(super) struct WriterLock {
    file: File,
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        if let Err(error) = self.file.unlock() {
            tracing::warn!(%error, "cannot release the wiki writer lock");
        }
    }
}

pub(super) fn acquire(root: &Path) -> Result<WriterLock, WikiObjectError> {
    let path = root.join("wiki-writer-v1.lock");
    layout::no_links(&path)?;
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path).map_err(crate::GitError::from)?;
    let metadata = file.metadata().map_err(crate::GitError::from)?;
    if !metadata.is_file() || metadata.len() != 0 {
        return Err(WikiObjectError::Binding);
    }
    file.try_lock().map_err(|error| match error {
        TryLockError::WouldBlock => WikiObjectError::Admission(AdmissionError::Busy),
        TryLockError::Error(error) => crate::GitError::from(error).into(),
    })?;
    Ok(WriterLock { file })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn completed_writer_releases_lock_even_with_a_duplicated_descriptor() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().canonicalize().unwrap();
        let writer = acquire(&path).unwrap();
        // A duplicated handle shares the Unix open file description, like the
        // descriptor inherited between fork and exec in an unrelated thread.
        let inherited = writer.file.try_clone().unwrap();
        assert!(matches!(acquire(&path), Err(WikiObjectError::Admission(AdmissionError::Busy))));
        drop(writer);
        let next = acquire(&path).expect("completed work releases ownership before its last duplicate closes");
        assert!(inherited.metadata().is_ok(), "the duplicate is still open");
        assert!(matches!(acquire(&path), Err(WikiObjectError::Admission(AdmissionError::Busy))));
        drop(next);
        drop(inherited);
        drop(acquire(&path).unwrap());
    }
}
