//! The file side of the rbit-actions artifact and cache stores: uploads stream into
//! `<store>/tmp`, are counted and hashed, then renamed into place; removals log anything but
//! a missing file. The decisions about when each of these happens are in
//! `rbit_actions::{artifacts, caches}`.

use std::{
    io,
    path::{Path, PathBuf},
};

use rbit_actions::archives::{Received, Refusal, StoreError, UPLOADS_DIRECTORY};
use rbit_core::actions::TaskId;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::runners::RunnerError;
use crate::ServiceError;

/// A store error as the runner protocol answers it.
pub(super) fn answer<E: Into<RunnerError>>(error: StoreError<E>) -> RunnerError {
    match error {
        StoreError::Port(error) => error.into(),
        StoreError::Refused(Refusal::BadRequest(message)) => RunnerError::bad_request(message),
        StoreError::Refused(Refusal::TooLarge(message)) => RunnerError::too_large(message),
        StoreError::Refused(Refusal::Conflict(message)) => RunnerError::conflict(message),
        StoreError::Refused(Refusal::NotFound(message)) => RunnerError::not_found(message),
    }
}

/// Streams `body` into a new file below `root`'s uploads, named after `task`; `None`, with
/// the partial file gone, once it passes `limit` bytes.
pub(super) async fn receive_upload(
    root: &Path,
    task: TaskId,
    mut body: impl AsyncRead + Unpin,
    limit: u64,
) -> Result<Option<Received<PathBuf>>, ServiceError> {
    let uploads = root.join(UPLOADS_DIRECTORY);
    tokio::fs::create_dir_all(&uploads).await.map_err(ServiceError::internal)?;
    let temporary = uploads.join(format!("{}-{}.part", task.0, super::tokens::random_name()?));
    match receive(&mut body, &temporary, limit).await {
        Ok(Some((size, sha256))) => Ok(Some(Received { temporary, size, sha256 })),
        Ok(None) => {
            discard(&temporary).await;
            Ok(None)
        }
        Err(error) => {
            discard(&temporary).await;
            Err(ServiceError::internal(error))
        }
    }
}

/// Moves `temporary` to `path`, creating its directory.
pub(super) async fn place(root: &Path, temporary: &Path, path: &Path) -> Result<(), ServiceError> {
    let placed = async {
        tokio::fs::create_dir_all(path.parent().unwrap_or(root)).await?;
        tokio::fs::rename(temporary, path).await
    }
    .await;
    placed.map_err(ServiceError::internal)
}

/// Removes an upload's temporary file, ignoring failures.
pub(super) async fn discard(temporary: &Path) {
    let _ = tokio::fs::remove_file(temporary).await;
}

/// Copies `body` into a new file at `path`, counting its size and SHA-256; `None` once it
/// passes `limit` bytes.
async fn receive(body: &mut (impl AsyncRead + Unpin), path: &Path, limit: u64) -> io::Result<Option<(u64, [u8; 32])>> {
    let mut file = tokio::fs::OpenOptions::new().write(true).create_new(true).open(path).await?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = body.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        size += read as u64;
        if size > limit {
            return Ok(None);
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).await?;
    }
    file.sync_all().await?;
    Ok(Some((size, hasher.finalize().into())))
}

/// Removes `path`, logging anything but its absence.
pub(super) async fn remove_file(path: &Path) {
    if let Err(error) = tokio::fs::remove_file(path).await
        && error.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(%error, path = %path.display(), "cannot remove an Actions file");
    }
}
