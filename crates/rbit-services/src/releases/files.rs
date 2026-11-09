//! Release file bytes on disk, the IO adapter of the release application.
//!
//! Bytes live at `<attachments_dir>/<repo_id>/<asset_id>`: numeric ids only, so no
//! user-supplied name ever becomes a path, and independent of the owner, so transfers
//! move nothing. An upload streams into `<attachments_dir>/tmp/<random>.part` while it is
//! hashed and measured and is abandoned (and deleted) the moment it exceeds the limit;
//! only a complete file gets a row, after which it is renamed into place (if the rename
//! fails the row is removed again). Deleting removes the row first, then the bytes, so a
//! crash in between leaves an orphaned file rather than a row without bytes. Row and file
//! operations hold the asset lock together, because asset ids can be reused.

use std::{
    collections::HashSet,
    fmt::Write as _,
    io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use rbit_core::{
    RepoId,
    release::{NewAsset, ReleaseAsset, ReleaseAssetId},
};
use rbit_repositories::releases::{AssetFiles, AssetUpload};
use tokio::io::AsyncRead;

use super::{AssetDownload, ReleaseService};
use crate::{
    ServiceError,
    uploads::{ReceivedUpload, receive},
};

/// The body of one upload request.
pub(super) struct UploadFiles<'a, B> {
    pub service: &'a ReleaseService,
    pub body: B,
}

impl<B: AsyncRead + Unpin + Send> AssetFiles for UploadFiles<'_, B> {
    type Received = (ReceivedUpload, PathBuf);
    type Error = ServiceError;

    async fn receive(&mut self, max_bytes: u64) -> Result<Option<(ReceivedUpload, PathBuf)>, ServiceError> {
        let tmp_dir = self.service.attachments_dir().join("tmp");
        tokio::fs::create_dir_all(&tmp_dir).await.map_err(ServiceError::internal)?;
        let tmp = tmp_dir.join(format!("{}.part", random_hex()?));
        match receive(&mut self.body, &tmp, max_bytes).await {
            Ok(Some(received)) => Ok(Some((received, tmp))),
            Ok(None) => {
                remove_quietly(&tmp).await;
                Ok(None)
            }
            Err(error) => {
                remove_quietly(&tmp).await;
                Err(ServiceError::internal(error))
            }
        }
    }

    async fn place(
        &mut self,
        upload: AssetUpload,
        (received, tmp): (ReceivedUpload, PathBuf),
        limit: u32,
    ) -> Result<Option<ReleaseAsset>, ServiceError> {
        let service = self.service.clone();
        // Once bytes are complete, own the row-and-file placement independently of the
        // HTTP request: cancellation must not leave a committed row without its file.
        self.service
            .inner
            .tasks
            .spawn(async move { service.place_asset(upload, received, tmp, limit).await })
            .await
            .map_err(ServiceError::internal)?
    }
}

impl ReleaseService {
    pub(super) fn attachments_dir(&self) -> PathBuf {
        self.inner.ctx.config.storage.attachments_dir()
    }

    /// `<attachments_dir>/<repo_id>/<asset_id>`.
    pub(super) fn asset_path(&self, repo: RepoId, asset: ReleaseAssetId) -> PathBuf {
        self.attachments_dir().join(repo.0.to_string()).join(asset.0.to_string())
    }

    async fn place_asset(
        &self,
        upload: AssetUpload,
        mut received: ReceivedUpload,
        tmp: PathBuf,
        limit: u32,
    ) -> Result<Option<ReleaseAsset>, ServiceError> {
        let _assets = self.inner.assets.write().await;
        let new = NewAsset {
            release_id: upload.release_id,
            repo_id: upload.repo_id,
            name: &upload.name,
            size: received.size,
            content_type: &upload.content_type,
            sha256: received.digest,
            uploader_id: upload.uploader_id,
            source: upload.source,
        };
        let inserted = self.store().insert_asset_limited(new, limit).await;
        let asset = match inserted {
            Ok(Some(asset)) => asset,
            Ok(None) => {
                remove_quietly(&tmp).await;
                return Ok(None);
            }
            Err(error) => {
                remove_quietly(&tmp).await;
                return Err(error.into());
            }
        };

        let target = self.asset_path(asset.repo_id, asset.id);
        let placed = async {
            if let Some(parent) = target.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::rename(&tmp, &target).await
        };
        if let Err(error) = placed.await {
            tracing::error!(%error, path = %target.display(), "cannot move an uploaded file into place; discarding it");
            remove_quietly(&tmp).await;
            if let Err(error) = self.store().delete_asset(asset.id).await {
                tracing::error!(%error, asset = asset.id.0, "cannot remove the row of a discarded upload");
            }
            return Err(ServiceError::internal(error));
        }
        received.placed();
        Ok(Some(asset))
    }

    /// Opens the bytes of `asset` and counts the download (best effort).
    pub(super) async fn open_file(&self, asset: ReleaseAsset) -> Result<AssetDownload, ServiceError> {
        let file = match tokio::fs::File::open(self.asset_path(asset.repo_id, asset.id)).await {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                tracing::error!(asset = asset.id.0, "the bytes of a release file are missing");
                return Err(ServiceError::NotFound);
            }
            Err(error) => return Err(ServiceError::internal(error)),
        };
        if let Err(error) = self.store().count_download(asset.id).await {
            tracing::warn!(%error, asset = asset.id.0, "cannot count a download");
        }
        Ok(AssetDownload { asset, file })
    }

    pub(super) async fn remove_file(&self, repo: RepoId, asset: ReleaseAssetId) {
        let path = self.asset_path(repo, asset);
        if let Err(error) = tokio::fs::remove_file(&path).await
            && error.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!(%error, path = %path.display(), "cannot remove a release file");
        }
    }

    /// Removes unreferenced files after repository deletion. A delayed event may name
    /// an id since reused by another repository, whose current files must survive.
    pub(crate) async fn remove_repository_files(&self, repo: RepoId) {
        let service = self.clone();
        let removal = self.inner.tasks.spawn(async move {
            let _assets = service.inner.assets.write().await;
            if let Err(error) = service.remove_unreferenced_files(repo).await {
                tracing::warn!(%error, repo = repo.0, "cannot remove the release files of a deleted repository");
            }
        });
        if let Err(error) = removal.await {
            tracing::warn!(%error, repo = repo.0, "release file removal task failed");
        }
    }

    async fn remove_unreferenced_files(&self, repo: RepoId) -> Result<(), ServiceError> {
        let keep: HashSet<_> = self.store().asset_ids_for_repo(repo).await?.into_iter().map(|id| id.0.to_string()).collect();
        let dir = self.attachments_dir().join(repo.0.to_string());
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(ServiceError::internal(error)),
        };
        while let Some(entry) = entries.next_entry().await.map_err(ServiceError::internal)? {
            if entry.file_name().to_str().is_some_and(|name| keep.contains(name)) {
                continue;
            }
            tokio::fs::remove_file(entry.path()).await.map_err(ServiceError::internal)?;
        }
        drop(entries);
        if keep.is_empty() {
            tokio::fs::remove_dir(&dir).await.map_err(ServiceError::internal)?;
        }
        Ok(())
    }
}

/// Removes `*.part` files in `dir` older than `age`; returns how many.
pub(super) async fn sweep_stale_uploads(dir: &Path, age: Duration) -> usize {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else { return 0 };
    let mut swept = 0;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "part") {
            continue;
        }
        let stale = entry
            .metadata()
            .await
            .and_then(|m| m.modified())
            .is_ok_and(|modified| SystemTime::now().duration_since(modified).unwrap_or_default() >= age);
        if stale && tokio::fs::remove_file(&path).await.is_ok() {
            swept += 1;
        }
    }
    swept
}

async fn remove_quietly(path: &Path) {
    let _ = tokio::fs::remove_file(path).await;
}

/// 128 random bits as hex, for temporary file names.
fn random_hex() -> Result<String, ServiceError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| ServiceError::internal(io::Error::other(e.to_string())))?;
    Ok(bytes.iter().fold(String::with_capacity(32), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    }))
}
