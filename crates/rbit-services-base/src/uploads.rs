//! Bounded, hashed uploads whose temporary files are cleaned up on cancellation.

use std::{
    io,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

/// A completed upload. Retains ownership of the temporary file until placement.
#[derive(Debug)]
pub(crate) struct ReceivedUpload {
    pub size: u64,
    pub digest: [u8; 32],
    cleanup: UploadCleanup,
}

impl ReceivedUpload {
    /// The temporary path has been moved or explicitly removed.
    pub fn placed(&mut self) {
        self.cleanup.0 = None;
    }
}

#[derive(Debug)]
struct UploadCleanup(Option<PathBuf>);

impl Drop for UploadCleanup {
    fn drop(&mut self) {
        let Some(path) = self.0.take() else { return };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = tokio::fs::remove_file(path).await;
            });
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Streams and hashes at most `limit` bytes. The cleanup owner is created together with
/// the file in the blocking task, so cancelling even the file-open future cannot leak
/// its result. A caller retains the returned owner until moving the file into place.
pub(crate) async fn receive(body: &mut (impl AsyncRead + Unpin), path: &Path, limit: u64) -> io::Result<Option<ReceivedUpload>> {
    let path = path.to_owned();
    let (file, cleanup) = tokio::task::spawn_blocking(move || {
        let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
        Ok::<_, io::Error>((file, UploadCleanup(Some(path))))
    })
    .await??;
    // Declared after cleanup, so the open handle is dropped before its cleanup owner.
    let mut file = tokio::fs::File::from_std(file);
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = body.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        if read as u64 > limit - size {
            return Ok(None);
        }
        size += read as u64;
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).await?;
    }
    file.flush().await?;
    file.sync_all().await?;
    Ok(Some(ReceivedUpload { size, digest: hasher.finalize().into(), cleanup }))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn wait_until(mut condition: impl FnMut() -> bool) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !condition() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn cancelling_an_upload_removes_its_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial.part");
        let target = path.clone();
        let (mut writer, mut reader) = tokio::io::duplex(64);
        let task = tokio::spawn(async move { receive(&mut reader, &target, 1024).await });
        writer.write_all(b"partial upload").await.unwrap();
        wait_until(|| std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() > 0)).await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        wait_until(|| !path.exists()).await;
    }

    #[tokio::test]
    async fn a_completed_upload_is_owned_until_placement() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("complete.part");
        let upload = receive(&mut b"hello".as_slice(), &path, 5).await.unwrap().unwrap();
        assert_eq!(upload.size, 5);
        assert_eq!(upload.digest, Sha256::digest(b"hello").as_slice());
        drop(upload);
        wait_until(|| !path.exists()).await;

        let mut upload = receive(&mut b"hello".as_slice(), &path, 5).await.unwrap().unwrap();
        let placed = dir.path().join("stored");
        tokio::fs::rename(&path, &placed).await.unwrap();
        upload.placed();
        drop(upload);
        assert_eq!(std::fs::read(placed).unwrap(), b"hello");
    }
}
