//! Releases: the composition adapter over [`rbit_repositories::releases`].
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! The release application decides visibility, permissions, validation, limits and
//! publication; this service passes it the opened repository's facts, maps its errors,
//! and owns the effects: tags reported like pushes ([`PushSource::Task`] for a task,
//! which starts no runs), [`DomainEvent::ReleasePublished`], tracing, and the attached
//! files on disk (see the `files` module).
//!
//! [`ReleaseService::spawn_background`] removes unreferenced files when a repository
//! is deleted, preserving current assets if its id was reused before cleanup, and at
//! start-up sweeps `tmp/*.part` files older than a day (uploads interrupted by a crash).

mod effects;
mod files;

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rbit_core::{
    Account, PushSource, User,
    release::{Release, ReleaseAsset, ReleaseAssetId, ReleaseId},
};
use rbit_repositories::releases::{AssetLimits, ReleaseAccess, ReleaseError, ReleaseWriter, Releases};
pub use rbit_repositories::releases::{RELEASES_PER_PAGE, ReleaseDetail, ReleaseInput, ReleasePage, human_bytes};
use rbit_storage::sqlite::releases::ReleaseStore;
use tokio::{
    io::AsyncRead,
    sync::{
        RwLock,
        broadcast::{Receiver, error::RecvError},
    },
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{DomainEvent, PushService, RepoContext, ServiceContext, ServiceError, actions::tokens::TaskGrant};
use effects::ReleaseEffects;
use files::UploadFiles;
#[cfg(test)]
use files::sweep_stale_uploads;
#[cfg(test)]
use rbit_core::release::{NewRelease, ReleaseSource};

// Actions: releases from workflows.
/// Who makes or changes a release: a person, or a running workflow task with its token.
#[derive(Debug, Clone, Copy)]
pub enum ReleaseActor<'a> {
    User(&'a User),
    Task(&'a TaskGrant),
}

impl<'a> From<&'a User> for ReleaseActor<'a> {
    fn from(user: &'a User) -> Self {
        Self::User(user)
    }
}

impl ReleaseActor<'_> {
    /// The facts the release application decides on.
    fn writer(self) -> ReleaseWriter {
        match self {
            Self::User(user) => ReleaseWriter::Person(user.id()),
            Self::Task(grant) => ReleaseWriter::Task { repo: grant.repo, contents_write: grant.contents().can_write() },
        }
    }

    /// Where the push of a tag it creates comes from; a task's starts no runs.
    fn push_source(self) -> PushSource {
        match self {
            Self::User(user) => PushSource::User(user.id()),
            Self::Task(grant) => PushSource::Task(grant.task),
        }
    }
}

/// How long [`ReleaseService::shutdown`] waits for background work.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
/// Interrupted uploads older than this are swept at start-up.
const STALE_UPLOAD_AGE: Duration = Duration::from_hours(24);

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

/// The opened repository's facts for the release application.
fn access(repo: &RepoContext) -> ReleaseAccess {
    ReleaseAccess { repo: repo.repo.repo.id, can_write: repo.access.can_write(), archived: repo.repo.repo.is_archived() }
}

fn service_error<E: Into<ServiceError>>(error: ReleaseError<E>) -> ServiceError {
    match error {
        ReleaseError::NotFound => ServiceError::NotFound,
        ReleaseError::Forbidden => ServiceError::Forbidden,
        ReleaseError::Archived => ServiceError::Archived,
        ReleaseError::Invalid(message) => ServiceError::Invalid(message),
        ReleaseError::Conflict(what) => ServiceError::Conflict(what),
        ReleaseError::Failure(error) => error.into(),
    }
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

    // ------------------------------------------------------------------ reading

    /// One page (1-based, clamped) of the repository's releases, newest first; drafts
    /// lead the list for writers and are absent for everyone else.
    pub async fn list(&self, repo: &RepoContext, page: u32) -> Result<ReleasePage, ServiceError> {
        Releases::list(&self.store(), access(repo), page).await.map_err(service_error)
    }

    /// `limit` releases from `offset`, ordered like [`Self::list`], and how many there
    /// are in all (for the API's pages).
    pub async fn page(&self, repo: &RepoContext, limit: u32, offset: u32) -> Result<(Vec<ReleaseDetail>, u64), ServiceError> {
        Releases::page(&self.store(), access(repo), limit, offset).await.map_err(service_error)
    }

    /// The accounts that wrote `releases` or uploaded their files, as far as they still
    /// exist (for API and webhook JSON).
    pub async fn people(&self, releases: &[ReleaseDetail]) -> Result<Vec<Account>, ServiceError> {
        Ok(self.inner.ctx.database.accounts().find_accounts(&Releases::people(releases)).await?)
    }

    /// The release of `tag`, if the viewer may see it.
    pub async fn by_tag(&self, repo: &RepoContext, tag: &str) -> Result<ReleaseDetail, ServiceError> {
        Releases::by_tag(&self.store(), access(repo), tag).await.map_err(service_error)
    }

    /// Release `id`, if the viewer may see it.
    pub async fn by_id(&self, repo: &RepoContext, id: ReleaseId) -> Result<ReleaseDetail, ServiceError> {
        Releases::by_id(&self.store(), access(repo), id).await.map_err(service_error)
    }

    /// The latest published release (full releases before pre-releases), for the sidebar.
    pub async fn latest(&self, repo: &RepoContext) -> Result<Option<Release>, ServiceError> {
        Ok(self.store().latest(repo.repo.repo.id).await?)
    }

    // ------------------------------------------------------------------ writing

    fn effects<'a>(&'a self, repo: &'a RepoContext, actor: ReleaseActor<'a>) -> ReleaseEffects<'a> {
        ReleaseEffects { service: self, repo, actor, store: self.store() }
    }

    /// Creates a release; publishes [`DomainEvent::ReleasePublished`] unless it is a draft.
    pub async fn create<'a>(
        &self,
        repo: &RepoContext,
        actor: impl Into<ReleaseActor<'a>>,
        input: ReleaseInput,
    ) -> Result<Release, ServiceError> {
        let actor = actor.into();
        Releases::create(&self.effects(repo, actor), access(repo), actor.writer(), input).await.map_err(service_error)
    }

    /// Edits release `id`. Turning a draft into a release publishes it (once: the first
    /// publication date is kept if it is made a draft again and re-published).
    pub async fn update<'a>(
        &self,
        repo: &RepoContext,
        actor: impl Into<ReleaseActor<'a>>,
        id: ReleaseId,
        input: ReleaseInput,
    ) -> Result<Release, ServiceError> {
        let actor = actor.into();
        Releases::update(&self.effects(repo, actor), access(repo), actor.writer(), id, input).await.map_err(service_error)
    }

    /// Deletes release `id` and its files (the tag stays).
    pub async fn delete(&self, repo: &RepoContext, id: ReleaseId) -> Result<(), ServiceError> {
        Releases::require_writer(access(repo)).map_err(service_error::<ServiceError>)?;
        let repo_id = repo.repo.repo.id;
        let service = self.clone();
        self.inner
            .tasks
            .spawn(async move {
                let _assets = service.inner.assets.write().await;
                let (release, assets) = Releases::delete(&service.store(), repo_id, id).await.map_err(service_error)?;
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

    /// Stores `body` as the file `name` of release `id` (see the `files` module for the
    /// procedure). Fails with [`ServiceError::Invalid`] for a bad name, too many files or
    /// a file over the size limit, and [`ServiceError::Conflict`] when the name is taken.
    pub async fn upload_asset<'a>(
        &self,
        repo: &RepoContext,
        actor: impl Into<ReleaseActor<'a>>,
        id: ReleaseId,
        name: &str,
        content_type: &str,
        body: impl AsyncRead + Unpin + Send,
    ) -> Result<ReleaseAsset, ServiceError> {
        let actor = actor.into();
        let config = &self.inner.ctx.config.releases;
        let limits = AssetLimits { max_assets: config.max_assets_per_release, max_bytes: config.max_asset_bytes };
        let mut files = UploadFiles { service: self, body };
        let effects = self.effects(repo, actor);
        let asset = Releases::upload(&effects, &mut files, access(repo), actor.writer(), id, name, content_type, limits)
            .await
            .map_err(service_error)?;
        tracing::info!(repo = %repo.repo.full_name(), name = asset.name, size = asset.size, "release file uploaded");
        Ok(asset)
    }

    /// Opens file `name` of the release of `tag` for download, if the viewer may see the
    /// release, and counts the download (best effort).
    pub async fn open_asset(&self, repo: &RepoContext, tag: &str, name: &str) -> Result<AssetDownload, ServiceError> {
        let _assets = self.inner.assets.read().await;
        let asset = Releases::asset_named(&self.store(), access(repo), tag, name).await.map_err(service_error)?;
        self.open_file(asset).await
    }

    /// File `id` of any release of `repo` and that release, if the viewer may see it.
    pub async fn asset(&self, repo: &RepoContext, id: ReleaseAssetId) -> Result<(ReleaseDetail, ReleaseAsset), ServiceError> {
        Releases::asset(&self.store(), access(repo), id).await.map_err(service_error)
    }

    /// Opens file `id` for download, like [`Self::open_asset`].
    pub async fn open_asset_by_id(&self, repo: &RepoContext, id: ReleaseAssetId) -> Result<AssetDownload, ServiceError> {
        let _assets = self.inner.assets.read().await;
        let (_, asset) = self.asset(repo, id).await?;
        self.open_file(asset).await
    }

    /// Deletes file `asset` of release `id`: the row, then the bytes.
    pub async fn delete_asset(&self, repo: &RepoContext, id: ReleaseId, asset: ReleaseAssetId) -> Result<(), ServiceError> {
        Releases::require_writer(access(repo)).map_err(service_error::<ServiceError>)?;
        let repo_id = repo.repo.repo.id;
        let service = self.clone();
        self.inner
            .tasks
            .spawn(async move {
                let _assets = service.inner.assets.write().await;
                let asset = Releases::delete_asset(&service.store(), repo_id, id, asset).await.map_err(service_error)?;
                service.remove_file(repo_id, asset.id).await;
                Ok(())
            })
            .await
            .map_err(ServiceError::internal)?
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
            let swept = files::sweep_stale_uploads(&tmp, STALE_UPLOAD_AGE).await;
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
                    author_id: Some(owner.id()),
                    source: ReleaseSource::User,
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
                author_id: Some(fixture.owner.id()),
                source: ReleaseSource::User,
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
