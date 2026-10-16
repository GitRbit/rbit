//! Releases: notes on a tag, and attached files stored under the attachments directory.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! # Rules
//!
//! * Anyone who can read the repository sees its published releases; drafts are visible
//!   to writers only (to everyone else a draft does not exist). Writers create, edit,
//!   publish and delete releases.
//! * A release names an **existing tag** of the repository, at most one release per tag.
//! * Publishing — creating a release as non-draft, or turning a draft into a release —
//!   publishes [`DomainEvent::ReleasePublished`] once.
//!
//! # Attached files
//!
//! Bytes live on disk at `<attachments_dir>/<repo_id>/<asset_id>`: numeric ids only, so
//! no user-supplied name ever becomes a path, and independent of the owner, so transfers
//! move nothing. An upload streams into `<attachments_dir>/tmp/<random>.part` while it is
//! hashed and measured and is abandoned (and deleted) the moment it exceeds
//! `[releases] max_asset_bytes`; only a complete file gets a row, after which it is
//! synced and renamed into place (if the rename fails the row is removed again).
//! Deleting a file removes the row first, then the bytes, so a crash in between leaves
//! an orphaned file rather than a row without bytes.
//!
//! [`ReleaseService::spawn_background`] removes a repository's directory when the
//! repository is deleted and, at start-up, sweeps `tmp/*.part` files older than a day
//! (uploads interrupted by a crash).

use std::{
    fmt::Write as _,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime},
};

use rustit_core::{
    RepoId, Timestamp, User,
    release::{Release, ReleaseAsset, ReleaseAssetId, ReleaseId, validate_asset_name, validate_release_name, validate_release_notes},
};
use rustit_storage::sqlite::releases::{NewAsset, NewRelease, ReleaseChanges, ReleaseStore};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    sync::broadcast::{Receiver, error::RecvError},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{DomainEvent, RepoContext, ServiceContext, ServiceError};

/// Releases per page of the releases list.
pub const RELEASES_PER_PAGE: u32 = 10;
/// How long [`ReleaseService::shutdown`] waits for background work.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
/// Interrupted uploads older than this are swept at start-up.
const STALE_UPLOAD_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// Longest declared content type kept for an attached file.
const MAX_CONTENT_TYPE_LEN: usize = 255;

/// Raw release form input, exactly as submitted.
#[derive(Debug, Clone, Default)]
pub struct ReleaseInput {
    pub tag_name: String,
    pub name: String,
    pub notes: String,
    pub is_draft: bool,
    pub is_prerelease: bool,
}

/// A release with its attached files.
#[derive(Debug, Clone)]
pub struct ReleaseDetail {
    pub release: Release,
    pub assets: Vec<ReleaseAsset>,
}

/// One page of the releases list.
#[derive(Debug, Clone)]
pub struct ReleasePage {
    pub releases: Vec<ReleaseDetail>,
    pub total: u64,
    /// 1-based page actually served.
    pub page: u32,
}

impl ReleasePage {
    pub fn page_count(&self) -> u32 {
        u32::try_from(self.total.div_ceil(u64::from(RELEASES_PER_PAGE)).max(1)).unwrap_or(u32::MAX)
    }
}

/// An attached file opened for download.
#[derive(Debug)]
pub struct AssetDownload {
    pub asset: ReleaseAsset,
    pub file: tokio::fs::File,
}

#[derive(Debug, Clone)]
pub struct ReleaseService {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    ctx: ServiceContext,
    shutdown: CancellationToken,
    tasks: TaskTracker,
    started: AtomicBool,
}

impl ReleaseService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self {
            inner: Arc::new(Inner { ctx, shutdown: CancellationToken::new(), tasks: TaskTracker::new(), started: AtomicBool::new(false) }),
        }
    }

    fn store(&self) -> ReleaseStore {
        self.inner.ctx.database.releases()
    }

    fn attachments_dir(&self) -> PathBuf {
        self.inner.ctx.config.storage.attachments_dir()
    }

    /// `<attachments_dir>/<repo_id>/<asset_id>`.
    fn asset_path(&self, repo: RepoId, asset: ReleaseAssetId) -> PathBuf {
        self.attachments_dir().join(repo.0.to_string()).join(asset.0.to_string())
    }

    // ------------------------------------------------------------------ reading

    /// One page (1-based, clamped) of the repository's releases, newest first; drafts
    /// lead the list for writers and are absent for everyone else.
    pub async fn list(&self, repo: &RepoContext, page: u32) -> Result<ReleasePage, ServiceError> {
        let include_drafts = repo.access.can_write();
        let repo_id = repo.repo.repo.id;
        let total = self.store().count(repo_id, include_drafts).await?;
        let mut result = ReleasePage { releases: Vec::new(), total, page: page.max(1) };
        result.page = result.page.min(result.page_count());
        let offset = (result.page - 1).saturating_mul(RELEASES_PER_PAGE);
        let releases = self.store().list(repo_id, include_drafts, RELEASES_PER_PAGE, offset).await?;
        let mut assets = self.store().assets_of(&releases.iter().map(|r| r.id).collect::<Vec<_>>()).await?;
        result.releases =
            releases.into_iter().map(|release| ReleaseDetail { assets: assets.remove(&release.id).unwrap_or_default(), release }).collect();
        Ok(result)
    }

    /// The release of `tag`, if the viewer may see it.
    pub async fn by_tag(&self, repo: &RepoContext, tag: &str) -> Result<ReleaseDetail, ServiceError> {
        let release = self.store().find_by_tag(repo.repo.repo.id, tag).await?;
        self.visible(repo, release).await
    }

    /// Release `id`, if the viewer may see it.
    pub async fn by_id(&self, repo: &RepoContext, id: ReleaseId) -> Result<ReleaseDetail, ServiceError> {
        let release = self.store().find(repo.repo.repo.id, id).await?;
        self.visible(repo, release).await
    }

    async fn visible(&self, repo: &RepoContext, release: Option<Release>) -> Result<ReleaseDetail, ServiceError> {
        let release = release.filter(|r| !r.is_draft || repo.access.can_write()).ok_or(ServiceError::NotFound)?;
        let assets = self.store().assets(release.id).await?;
        Ok(ReleaseDetail { release, assets })
    }

    /// The latest published release (full releases before pre-releases), for the sidebar.
    pub async fn latest(&self, repo: &RepoContext) -> Result<Option<Release>, ServiceError> {
        Ok(self.store().latest(repo.repo.repo.id).await?)
    }

    // ------------------------------------------------------------------ writing

    fn require_writer(repo: &RepoContext) -> Result<(), ServiceError> {
        if repo.access.can_write() { Ok(()) } else { Err(ServiceError::Forbidden) }
    }

    /// Validates `input` against the repository: the tag must exist, the title and notes
    /// fit. Returns the trimmed tag and the title (the tag when left empty).
    async fn validated(repo: &RepoContext, input: &ReleaseInput) -> Result<(String, String), ServiceError> {
        let tag = input.tag_name.trim();
        if tag.is_empty() {
            return Err(ServiceError::Invalid("choose the tag this release is for".to_owned()));
        }
        let snapshot = repo.reader.ref_snapshot().await?;
        if snapshot.get(&format!("refs/tags/{tag}")).is_none() {
            return Err(ServiceError::Invalid(format!("there is no tag called {tag}; push it first")));
        }
        let name = match input.name.trim() {
            "" => tag,
            name => name,
        };
        validate_release_name(name).map_err(ServiceError::Invalid)?;
        validate_release_notes(&input.notes).map_err(ServiceError::Invalid)?;
        Ok((tag.to_owned(), name.to_owned()))
    }

    /// Creates a release; publishes [`DomainEvent::ReleasePublished`] unless it is a draft.
    pub async fn create(&self, repo: &RepoContext, actor: &User, input: ReleaseInput) -> Result<Release, ServiceError> {
        Self::require_writer(repo)?;
        let (tag, name) = Self::validated(repo, &input).await?;
        let repo_id = repo.repo.repo.id;
        let id = self
            .store()
            .create(NewRelease {
                repo_id,
                tag_name: &tag,
                name: &name,
                notes: &input.notes,
                is_draft: input.is_draft,
                is_prerelease: input.is_prerelease,
                author_id: actor.id(),
            })
            .await?;
        if !input.is_draft {
            self.inner.ctx.events.publish(DomainEvent::ReleasePublished { repo: repo_id, release: id, actor: actor.id() });
        }
        tracing::info!(repo = %repo.repo.full_name(), tag, draft = input.is_draft, "release created");
        self.store().find(repo_id, id).await?.ok_or(ServiceError::NotFound)
    }

    /// Edits release `id`. Turning a draft into a release publishes it (once: the first
    /// publication date is kept if it is made a draft again and re-published).
    pub async fn update(&self, repo: &RepoContext, actor: &User, id: ReleaseId, input: ReleaseInput) -> Result<Release, ServiceError> {
        Self::require_writer(repo)?;
        let repo_id = repo.repo.repo.id;
        let current = self.store().find(repo_id, id).await?.ok_or(ServiceError::NotFound)?;
        let (tag, name) = Self::validated(repo, &input).await?;
        let publishing = current.published_at.is_none() && !input.is_draft;
        self.store()
            .update(
                id,
                ReleaseChanges {
                    tag_name: &tag,
                    name: &name,
                    notes: &input.notes,
                    is_draft: input.is_draft,
                    is_prerelease: input.is_prerelease,
                    published_at: (!input.is_draft).then(Timestamp::now),
                },
            )
            .await?;
        if publishing {
            self.inner.ctx.events.publish(DomainEvent::ReleasePublished { repo: repo_id, release: id, actor: actor.id() });
        }
        self.store().find(repo_id, id).await?.ok_or(ServiceError::NotFound)
    }

    /// Deletes release `id` and its files (the tag stays).
    pub async fn delete(&self, repo: &RepoContext, id: ReleaseId) -> Result<(), ServiceError> {
        Self::require_writer(repo)?;
        let repo_id = repo.repo.repo.id;
        let release = self.store().find(repo_id, id).await?.ok_or(ServiceError::NotFound)?;
        let assets = self.store().assets(release.id).await?;
        self.store().delete(release.id).await?;
        for asset in assets {
            self.remove_file(repo_id, asset.id).await;
        }
        tracing::info!(repo = %repo.repo.full_name(), tag = release.tag_name, "release deleted");
        Ok(())
    }

    // ------------------------------------------------------------------ attached files

    /// Stores `body` as the file `name` of release `id` (see the module docs for the
    /// procedure). Fails with [`ServiceError::Invalid`] for a bad name, too many files or
    /// a file over the size limit, and [`ServiceError::Conflict`] when the name is taken.
    pub async fn upload_asset(
        &self,
        repo: &RepoContext,
        actor: &User,
        id: ReleaseId,
        name: &str,
        content_type: &str,
        mut body: impl AsyncRead + Unpin + Send,
    ) -> Result<ReleaseAsset, ServiceError> {
        Self::require_writer(repo)?;
        let repo_id = repo.repo.repo.id;
        let release = self.store().find(repo_id, id).await?.ok_or(ServiceError::NotFound)?;
        validate_asset_name(name).map_err(ServiceError::Invalid)?;
        let config = &self.inner.ctx.config.releases;
        if self.store().count_assets(release.id).await? >= config.max_assets_per_release {
            return Err(ServiceError::Invalid(format!("a release can have at most {} files", config.max_assets_per_release)));
        }
        if self.store().find_asset(release.id, name).await?.is_some() {
            return Err(ServiceError::Conflict("a file with this name"));
        }

        let tmp_dir = self.attachments_dir().join("tmp");
        tokio::fs::create_dir_all(&tmp_dir).await.map_err(ServiceError::internal)?;
        let tmp = tmp_dir.join(format!("{}.part", random_hex()?));
        let received = receive(&mut body, &tmp, config.max_asset_bytes).await;
        let (size, sha256) = match received {
            Ok(Some(received)) => received,
            Ok(None) => {
                remove_quietly(&tmp).await;
                return Err(ServiceError::Invalid(format!("files can be at most {}", human_bytes(config.max_asset_bytes))));
            }
            Err(error) => {
                remove_quietly(&tmp).await;
                return Err(ServiceError::internal(error));
            }
        };

        let content_type: String = content_type.trim().chars().filter(|c| !c.is_control()).take(MAX_CONTENT_TYPE_LEN).collect();
        let inserted = self
            .store()
            .insert_asset(NewAsset {
                release_id: release.id,
                repo_id,
                name,
                size,
                content_type: if content_type.is_empty() { "application/octet-stream" } else { &content_type },
                sha256,
                uploader_id: actor.id(),
            })
            .await;
        let asset = match inserted {
            Ok(asset) => asset,
            Err(error) => {
                remove_quietly(&tmp).await;
                return Err(error.into());
            }
        };

        let target = self.asset_path(repo_id, asset.id);
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
        tracing::info!(repo = %repo.repo.full_name(), tag = release.tag_name, name, size, "release file uploaded");
        Ok(asset)
    }

    /// Opens file `name` of the release of `tag` for download, if the viewer may see the
    /// release, and counts the download (best effort).
    pub async fn open_asset(&self, repo: &RepoContext, tag: &str, name: &str) -> Result<AssetDownload, ServiceError> {
        let release = self.by_tag(repo, tag).await?.release;
        let asset = self.store().find_asset(release.id, name).await?.ok_or(ServiceError::NotFound)?;
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

    /// Deletes file `asset` of release `id`: the row, then the bytes.
    pub async fn delete_asset(&self, repo: &RepoContext, id: ReleaseId, asset: ReleaseAssetId) -> Result<(), ServiceError> {
        Self::require_writer(repo)?;
        let repo_id = repo.repo.repo.id;
        let release = self.store().find(repo_id, id).await?.ok_or(ServiceError::NotFound)?;
        let asset = self.store().assets(release.id).await?.into_iter().find(|a| a.id == asset).ok_or(ServiceError::NotFound)?;
        self.store().delete_asset(asset.id).await?;
        self.remove_file(repo_id, asset.id).await;
        Ok(())
    }

    async fn remove_file(&self, repo: RepoId, asset: ReleaseAssetId) {
        let path = self.asset_path(repo, asset);
        if let Err(error) = tokio::fs::remove_file(&path).await
            && error.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!(%error, path = %path.display(), "cannot remove a release file");
        }
    }

    // ------------------------------------------------------------------ background

    /// Starts the clean-up subscriber (files of deleted repositories) and sweeps
    /// interrupted uploads. Called once at start-up; later calls do nothing.
    pub fn spawn_background(&self) {
        if self.inner.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let events = self.inner.ctx.events.subscribe();
        let service = self.clone();
        self.inner.tasks.spawn(async move { service.listen(events).await });
        let sweeper = self.clone();
        self.inner.tasks.spawn(async move {
            let tmp = sweeper.attachments_dir().join("tmp");
            let swept = sweep_stale_uploads(&tmp, STALE_UPLOAD_AGE).await;
            if swept > 0 {
                tracing::info!(swept, "removed interrupted release uploads");
            }
        });
    }

    /// Stops background work and waits (bounded) for it to finish. Called from
    /// [`crate::Services::shutdown`] before the stores close.
    pub async fn shutdown(&self) {
        self.inner.shutdown.cancel();
        self.inner.tasks.close();
        if tokio::time::timeout(SHUTDOWN_GRACE, self.inner.tasks.wait()).await.is_err() {
            tracing::warn!("release clean-up did not stop within {SHUTDOWN_GRACE:?}");
        }
    }

    async fn listen(self, mut events: Receiver<DomainEvent>) {
        loop {
            let event = tokio::select! {
                () = self.inner.shutdown.cancelled() => break,
                event = events.recv() => event,
            };
            match event {
                Ok(DomainEvent::RepositoryDeleted { repo, .. }) => self.remove_repository_files(repo).await,
                Ok(_) => {}
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "release clean-up fell behind; files of deleted repositories may remain");
                }
                Err(RecvError::Closed) => break,
            }
        }
    }

    /// Removes `<attachments_dir>/<repo_id>` (every file of the repository's releases).
    pub(crate) async fn remove_repository_files(&self, repo: RepoId) {
        let dir = self.attachments_dir().join(repo.0.to_string());
        match tokio::fs::remove_dir_all(&dir).await {
            Ok(()) => tracing::info!(repo = repo.0, "removed the release files of a deleted repository"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(%error, path = %dir.display(), "cannot remove the release files of a deleted repository"),
        }
    }
}

/// Streams `body` into a new file at `path`, hashing it. `Ok(None)` once it exceeds
/// `limit` bytes (the caller deletes the partial file).
async fn receive(body: &mut (impl AsyncRead + Unpin), path: &Path, limit: u64) -> io::Result<Option<(u64, [u8; 32])>> {
    let mut file = tokio::fs::OpenOptions::new().write(true).create_new(true).open(path).await?;
    let mut hasher = Sha256::new();
    let mut size: u64 = 0;
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
    file.flush().await?;
    file.sync_all().await?;
    Ok(Some((size, hasher.finalize().into())))
}

/// Removes `*.part` files in `dir` older than `age`; returns how many.
async fn sweep_stale_uploads(dir: &Path, age: Duration) -> usize {
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

/// `512 MiB`, `10 KiB` — for limit messages.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [(u64, &str); 3] = [(1 << 30, "GiB"), (1 << 20, "MiB"), (1 << 10, "KiB")];
    for (unit, label) in UNITS {
        if bytes >= unit && bytes.is_multiple_of(unit) {
            return format!("{} {label}", bytes / unit);
        }
    }
    format!("{bytes} bytes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_read_naturally() {
        assert_eq!(human_bytes(512 * 1024 * 1024), "512 MiB");
        assert_eq!(human_bytes(10 * 1024), "10 KiB");
        assert_eq!(human_bytes(1000), "1000 bytes");
    }

    #[tokio::test]
    async fn stale_uploads_are_swept_and_others_kept() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.part"), "x").unwrap();
        std::fs::write(dir.path().join("keep.txt"), "x").unwrap();
        assert_eq!(sweep_stale_uploads(dir.path(), Duration::from_secs(3600)).await, 0, "fresh uploads stay");
        assert_eq!(sweep_stale_uploads(dir.path(), Duration::ZERO).await, 1);
        assert!(dir.path().join("keep.txt").exists());
        assert_eq!(sweep_stale_uploads(&dir.path().join("missing"), Duration::ZERO).await, 0);
    }
}
