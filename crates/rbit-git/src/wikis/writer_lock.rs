//! One held cooperating-writer lock, not filesystem integrity or durability proof.
use super::{WikiObjectError, layout};
use rbit_kernel::AdmissionError;
use std::{
    fs::{File, OpenOptions, TryLockError},
    path::Path,
};

pub(super) fn acquire(root: &Path) -> Result<File, WikiObjectError> {
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
    Ok(file)
}
