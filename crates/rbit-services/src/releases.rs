//! Releases: notes on a tag, and attached files stored under the attachments directory.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! # Rules
//!
//! * Anyone who can read the repository sees its published releases; drafts are visible
//!   to writers only (to everyone else a draft does not exist). Writers create, edit,
//!   publish and delete releases.
//! * A release names a tag of the repository, at most one release per tag. A new
//!   release may create its tag from a branch or commit (`ReleaseInput::target`); the
//!   new tag is reported like a push.
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
//! [`ReleaseService::spawn_background`] removes unreferenced files when a repository
//! is deleted, preserving current assets if its id was reused before cleanup, and at
//! start-up sweeps `tmp/*.part` files older than a day (uploads interrupted by a crash).

use std::{
    collections::HashSet,
    fmt::Write as _,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime},
};

use rbit_core::{
    Account, RepoId, Timestamp, User,
    release::{Release, ReleaseAsset, ReleaseAssetId, ReleaseId, validate_asset_name, validate_release_name, validate_release_notes},
};
use rbit_git::GitError;
use rbit_storage::sqlite::releases::{NewAsset, NewRelease, ReleaseChanges, ReleaseStore};
use tokio::{
    io::AsyncRead,
    sync::{
        RwLock,
        broadcast::{Receiver, error::RecvError},
    },
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::uploads::{ReceivedUpload, receive};
use crate::{DomainEvent, PushService, RepoContext, ServiceContext, ServiceError};

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
    /// When creating a release for a tag that does not exist yet: the branch (or commit)
    /// to create it from. Empty means the tag must already exist.
    pub target: String,
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
    /// Tags created for releases are reported like pushes.
    pushes: PushService,
    /// Asset ids can be reused after deletion. Keep metadata and file operations
    /// together so old operations cannot act on replacement bytes.
    assets: RwLock<()>,
    shutdown: CancellationToken,
    tasks: TaskTracker,
    started: AtomicBool,
}

impl ReleaseService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self {
            inner: Arc::new(Inner {
                pushes: PushService::new(ctx.clone()),
                assets: RwLock::new(()),
                ctx,
                shutdown: CancellationToken::new(),
                tasks: TaskTracker::new(),
                started: AtomicBool::new(false),
            }),
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

    /// `limit` releases from `offset`, ordered like [`Self::list`], and how many there
    /// are in all (for the API's pages).
    pub async fn page(&self, repo: &RepoContext, limit: u32, offset: u32) -> Result<(Vec<ReleaseDetail>, u64), ServiceError> {
        let include_drafts = repo.access.can_write();
        let repo_id = repo.repo.repo.id;
        let total = self.store().count(repo_id, include_drafts).await?;
        let releases = self.store().list(repo_id, include_drafts, limit, offset).await?;
        let mut assets = self.store().assets_of(&releases.iter().map(|r| r.id).collect::<Vec<_>>()).await?;
        let releases =
            releases.into_iter().map(|release| ReleaseDetail { assets: assets.remove(&release.id).unwrap_or_default(), release }).collect();
        Ok((releases, total))
    }

    /// The accounts that wrote `releases` or uploaded their files, as far as they still
    /// exist (for API and webhook JSON).
    pub async fn people(&self, releases: &[ReleaseDetail]) -> Result<Vec<Account>, ServiceError> {
        let mut ids: Vec<_> = releases
            .iter()
            .flat_map(|detail| {
                detail.release.author.iter().map(|author| author.id).chain(detail.assets.iter().filter_map(|a| a.uploader_id))
            })
            .collect();
        ids.sort_unstable();
        ids.dedup();
        Ok(self.inner.ctx.database.accounts().find_accounts(&ids).await?)
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

    /// Checks the text fields of `input`. Returns the trimmed tag and the title (the tag
    /// when left empty).
    fn validated_text(input: &ReleaseInput) -> Result<(String, String), ServiceError> {
        let tag = input.tag_name.trim();
        if tag.is_empty() {
            return Err(ServiceError::Invalid("choose the tag this release is for".to_owned()));
        }
        let name = match input.name.trim() {
            "" => tag,
            name => name,
        };
        validate_release_name(name).map_err(ServiceError::Invalid)?;
        validate_release_notes(&input.notes).map_err(ServiceError::Invalid)?;
        Ok((tag.to_owned(), name.to_owned()))
    }

    async fn tag_exists(repo: &RepoContext, tag: &str) -> Result<bool, ServiceError> {
        Ok(repo.reader.ref_snapshot().await?.get(&format!("refs/tags/{tag}")).is_some())
    }

    /// [`Self::validated_text`], plus: the tag must exist.
    async fn validated(repo: &RepoContext, input: &ReleaseInput) -> Result<(String, String), ServiceError> {
        let (tag, name) = Self::validated_text(input)?;
        if !Self::tag_exists(repo, &tag).await? {
            return Err(ServiceError::Invalid(format!("there is no tag called {tag}; push it first")));
        }
        Ok((tag, name))
    }

    /// Creates tag `tag` at `target` for a new release, reported like a push of the tag
    /// ([`DomainEvent::RepositoryPushed`]) so caches and webhooks see it.
    async fn create_tag(&self, repo: &RepoContext, actor: &User, tag: &str, target: &str) -> Result<(), ServiceError> {
        let pending = self.inner.pushes.begin(repo, Some(actor.id())).await;
        match repo.reader.create_tag(tag.to_owned(), target.to_owned()).await {
            Ok(_) => {
                self.inner.pushes.complete(pending).await;
                tracing::info!(repo = %repo.repo.full_name(), tag, target, "tag created for a release");
                Ok(())
            }
            Err(GitError::InvalidBranchName(_)) => Err(ServiceError::Invalid(format!("{tag} is not a valid tag name"))),
            Err(GitError::AlreadyExists) => Err(ServiceError::Conflict("the tag")),
            Err(GitError::NotFound(_)) => Err(ServiceError::Invalid(format!("there is no branch or commit called {target}"))),
            Err(other) => Err(other.into()),
        }
    }

    /// Creates a release; publishes [`DomainEvent::ReleasePublished`] unless it is a draft.
    pub async fn create(&self, repo: &RepoContext, actor: &User, input: ReleaseInput) -> Result<Release, ServiceError> {
        Self::require_writer(repo)?;
        let (tag, name) = Self::validated_text(&input)?;
        let repo_id = repo.repo.repo.id;
        if !Self::tag_exists(repo, &tag).await? {
            let target = input.target.trim();
            if target.is_empty() {
                return Err(ServiceError::Invalid(format!("there is no tag called {tag}; push it or choose a branch to create it from")));
            }
            if self.store().find_by_tag(repo_id, &tag).await?.is_some() {
                return Err(ServiceError::Conflict("a release for this tag"));
            }
            self.create_tag(repo, actor, &tag, target).await?;
        }
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
        let service = self.clone();
        self.inner
            .tasks
            .spawn(async move {
                let _assets = service.inner.assets.write().await;
                let release = service.store().find(repo_id, id).await?.ok_or(ServiceError::NotFound)?;
                let assets = service.store().assets(release.id).await?;
                service.store().delete(release.id).await?;
                for asset in assets {
                    service.remove_file(repo_id, asset.id).await;
                }
                tracing::info!(repo = repo_id.0, tag = release.tag_name, "release deleted");
                Ok(())
            })
            .await
            .map_err(ServiceError::internal)?
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
        let received = match received {
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
        let name = name.to_owned();
        let uploader_id = actor.id();
        let limit = config.max_assets_per_release;
        let service = self.clone();
        // Once bytes are complete, own the row-and-file placement independently of the
        // HTTP request: cancellation must not leave a committed row without its file.
        let asset = self
            .inner
            .tasks
            .spawn(async move {
                let new = NewAsset {
                    release_id: release.id,
                    repo_id,
                    name: &name,
                    size: received.size,
                    content_type: if content_type.is_empty() { "application/octet-stream" } else { &content_type },
                    sha256: received.digest,
                    uploader_id,
                };
                service.place_asset(new, received, tmp, limit).await
            })
            .await
            .map_err(ServiceError::internal)??;
        tracing::info!(repo = %repo.repo.full_name(), name = asset.name, size = asset.size, "release file uploaded");
        Ok(asset)
    }

    async fn place_asset(
        &self,
        new: NewAsset<'_>,
        mut received: ReceivedUpload,
        tmp: PathBuf,
        limit: u32,
    ) -> Result<ReleaseAsset, ServiceError> {
        let _assets = self.inner.assets.write().await;
        let inserted = self.store().insert_asset_limited(new, limit).await;
        let asset = match inserted {
            Ok(Some(asset)) => asset,
            Ok(None) => {
                remove_quietly(&tmp).await;
                return Err(ServiceError::Invalid(format!("a release can have at most {limit} files")));
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
        Ok(asset)
    }

    /// Opens file `name` of the release of `tag` for download, if the viewer may see the
    /// release, and counts the download (best effort).
    pub async fn open_asset(&self, repo: &RepoContext, tag: &str, name: &str) -> Result<AssetDownload, ServiceError> {
        let _assets = self.inner.assets.read().await;
        let release = self.by_tag(repo, tag).await?.release;
        let asset = self.store().find_asset(release.id, name).await?.ok_or(ServiceError::NotFound)?;
        self.open_file(asset).await
    }

    /// File `id` of any release of `repo` and that release, if the viewer may see it.
    pub async fn asset(&self, repo: &RepoContext, id: ReleaseAssetId) -> Result<(ReleaseDetail, ReleaseAsset), ServiceError> {
        let asset = self.store().find_asset_by_id(repo.repo.repo.id, id).await?.ok_or(ServiceError::NotFound)?;
        let release = self.by_id(repo, asset.release_id).await?;
        Ok((release, asset))
    }

    /// Opens file `id` for download, like [`Self::open_asset`].
    pub async fn open_asset_by_id(&self, repo: &RepoContext, id: ReleaseAssetId) -> Result<AssetDownload, ServiceError> {
        let _assets = self.inner.assets.read().await;
        let (_, asset) = self.asset(repo, id).await?;
        self.open_file(asset).await
    }

    /// Opens the bytes of `asset` and counts the download (best effort).
    async fn open_file(&self, asset: ReleaseAsset) -> Result<AssetDownload, ServiceError> {
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
        let service = self.clone();
        self.inner
            .tasks
            .spawn(async move {
                let _assets = service.inner.assets.write().await;
                let release = service.store().find(repo_id, id).await?.ok_or(ServiceError::NotFound)?;
                let asset = service.store().assets(release.id).await?.into_iter().find(|a| a.id == asset).ok_or(ServiceError::NotFound)?;
                service.store().delete_asset(asset.id).await?;
                service.remove_file(repo_id, asset.id).await;
                Ok(())
            })
            .await
            .map_err(ServiceError::internal)?
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

    struct Fixture {
        _dir: tempfile::TempDir,
        service: ReleaseService,
        owner: User,
        repo: RepoContext,
        release: ReleaseId,
    }

    impl Fixture {
        async fn new() -> Self {
            use rbit_core::{Config, Visibility};
            use rbit_storage::{Database, KvStore, NewUser};

            let dir = tempfile::tempdir().unwrap();
            let mut config = Config::default();
            config.storage.data_dir = dir.path().join("data");
            std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
            let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
            let kv = KvStore::open(&config.storage.kv_path()).unwrap();
            let services = crate::Services::new(Arc::new(config), &database, &kv).await.unwrap();
            let owner = database
                .accounts()
                .create_user(NewUser {
                    name: "alice".parse().unwrap(),
                    email: "alice@example.com".parse().unwrap(),
                    display_name: None,
                    password_hash: "test".to_owned(),
                    is_admin: false,
                })
                .await
                .unwrap();
            services
                .repositories
                .create(
                    &owner,
                    crate::CreateRepository { name: "assets".to_owned(), description: String::new(), visibility: Visibility::Private },
                )
                .await
                .unwrap();
            let repo = services.repositories.open(Some(&owner), "alice", "assets").await.unwrap();
            let release = database
                .releases()
                .create(NewRelease {
                    repo_id: repo.repo.repo.id,
                    tag_name: "v1",
                    name: "v1",
                    notes: "",
                    is_draft: false,
                    is_prerelease: false,
                    author_id: owner.id(),
                })
                .await
                .unwrap();
            Self { _dir: dir, service: services.releases, owner, repo, release }
        }

        async fn upload(&self, name: &str, bytes: &[u8]) -> ReleaseAsset {
            self.service.upload_asset(&self.repo, &self.owner, self.release, name, "application/octet-stream", bytes).await.unwrap()
        }
    }

    async fn wait_for_tasks(service: &ReleaseService, count: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while service.inner.tasks.len() != count {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn completed_asset_upload_survives_request_cancellation() {
        let fixture = Fixture::new().await;
        let assets = fixture.service.inner.assets.write().await;
        let service = fixture.service.clone();
        let repo = fixture.repo.clone();
        let owner = fixture.owner.clone();
        let id = fixture.release;
        let request = tokio::spawn(async move {
            service.upload_asset(&repo, &owner, id, "file.bin", "application/octet-stream", b"complete".as_slice()).await
        });
        wait_for_tasks(&fixture.service, 1).await;
        request.abort();
        assert!(request.await.unwrap_err().is_cancelled());
        drop(assets);
        wait_for_tasks(&fixture.service, 0).await;

        let asset = fixture.service.store().find_asset(id, "file.bin").await.unwrap().unwrap();
        assert_eq!(tokio::fs::read(fixture.service.asset_path(asset.repo_id, asset.id)).await.unwrap(), b"complete");
        assert_eq!(std::fs::read_dir(fixture.service.attachments_dir().join("tmp")).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn cancelled_deletion_keeps_row_and_file_together_before_id_reuse() {
        let fixture = Fixture::new().await;
        let old = fixture.upload("old.bin", b"old").await;
        let assets = fixture.service.inner.assets.write().await;
        let service = fixture.service.clone();
        let repo = fixture.repo.clone();
        let id = fixture.release;
        let old_id = old.id;
        let request = tokio::spawn(async move { service.delete_asset(&repo, id, old_id).await });
        wait_for_tasks(&fixture.service, 1).await;
        request.abort();
        assert!(request.await.unwrap_err().is_cancelled());
        assert!(fixture.service.store().find_asset(id, "old.bin").await.unwrap().is_some(), "row stays until file access is exclusive");
        drop(assets);
        wait_for_tasks(&fixture.service, 0).await;
        assert!(fixture.service.store().find_asset(id, "old.bin").await.unwrap().is_none());
        assert!(!fixture.service.asset_path(old.repo_id, old.id).exists());

        let replacement = fixture.upload("new.bin", b"replacement").await;
        assert_eq!(replacement.id, old_id, "exercise SQLite's asset id reuse");
        let download = fixture.service.open_asset(&fixture.repo, "v1", "new.bin").await.unwrap();
        assert_eq!(download.asset.name, "new.bin");
        assert_eq!(tokio::fs::read(fixture.service.asset_path(replacement.repo_id, replacement.id)).await.unwrap(), b"replacement");
    }

    #[tokio::test]
    async fn delayed_repository_cleanup_preserves_reused_ids_current_assets() {
        use rbit_core::Visibility;
        use rbit_storage::NewRepository;

        let fixture = Fixture::new().await;
        let old = fixture.upload("old.bin", b"old").await;
        let orphan = fixture.upload("orphan.bin", b"orphan").await;
        let database = &fixture.service.inner.ctx.database;
        let repo_id = fixture.repo.repo.repo.id;
        database.repo_settings().delete(repo_id).await.unwrap();
        let replacement = database
            .repositories()
            .create(NewRepository {
                owner_id: fixture.owner.id(),
                name: "replacement".parse().unwrap(),
                description: None,
                visibility: Visibility::Private,
                default_branch: "main".to_owned(),
            })
            .await
            .unwrap();
        assert_eq!(replacement.id, repo_id, "exercise repository id reuse");
        let release = database
            .releases()
            .create(NewRelease {
                repo_id,
                tag_name: "new",
                name: "new",
                notes: "",
                is_draft: false,
                is_prerelease: false,
                author_id: fixture.owner.id(),
            })
            .await
            .unwrap();
        let mut repo = fixture.repo.clone();
        repo.repo.repo = replacement;
        let current = fixture
            .service
            .upload_asset(&repo, &fixture.owner, release, "current.bin", "application/octet-stream", b"current".as_slice())
            .await
            .unwrap();
        assert_eq!(current.id, old.id, "the file path is reused too");

        fixture.service.remove_repository_files(repo_id).await;
        assert_eq!(tokio::fs::read(fixture.service.asset_path(repo_id, current.id)).await.unwrap(), b"current");
        assert!(!fixture.service.asset_path(repo_id, orphan.id).exists(), "unreferenced old files are removed");
    }

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
