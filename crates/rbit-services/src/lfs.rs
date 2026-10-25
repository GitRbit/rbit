//! Git Large File Storage: objects stored outside Git, the batch API's decisions, the
//! signed transfer tokens, and file locks ([`locks`]).
//!
//! Milestone 3 workstream L (Git LFS) owns this file.
//!
//! # Objects
//!
//! An object's bytes live at `<data_dir>/lfs/<aa>/<bb>/<oid>`, once however many
//! repositories use it; `lfs_repo_objects` says which repositories may. An upload
//! ([`LfsService::store`]) streams into `<data_dir>/tmp/lfs/<random>.part` while it is
//! hashed and measured, is refused unless size and SHA-256 match the announced object,
//! and is then renamed into place (or discarded when the bytes are already there) and
//! linked to the repository. Bytes are placed before the row is written, so a crash
//! leaves at worst an unreferenced file, never a row without bytes.
//!
//! # Which objects a repository may use
//!
//! A repository can download the objects linked to it. Because forks share history,
//! an object linked to another repository of the *fork network* also counts when the
//! requester may read that repository; it is then linked to this repository too, so it
//! stays available if the other one is deleted. Forks copy their parent's links when
//! they are created ([`DomainEvent::RepositoryForked`]). The same rule decides whether an
//! upload can be skipped: an object stored for an unrelated repository must be uploaded
//! again (proving the client has the content) before it is linked, so knowing an object
//! id never grants its content.
//!
//! # Tokens
//!
//! Batch responses and `git-lfs-authenticate` hand out short-lived signed headers
//! ([`tokens`]); [`LfsService::open_granted`] turns one back into a repository context,
//! recomputing the holder's access, so revoked access ends a token early.
//!
//! # Background work
//!
//! [`LfsService::spawn_background`] copies links to new forks, sweeps interrupted uploads
//! at start-up, and, on the `[operations]` maintenance schedule, removes objects no
//! repository has linked for [`ORPHAN_GRACE`] ([`LfsService::prune_orphans`]). Deleting a
//! repository unlinks its objects (the links cascade).

pub mod locks;
pub mod tokens;

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

use rbit_auth::{Grants, repo_access};
use rbit_core::{
    RepoAccess, RepoId, Timestamp, User,
    lfs::{LfsOid, LfsOperation},
    repository::OwnedRepository,
};
use rbit_storage::sqlite::lfs::LfsStore;
use tokio::{
    io::AsyncRead,
    sync::{
        Mutex,
        broadcast::{Receiver, error::RecvError},
    },
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use self::tokens::TokenKey;
pub use self::tokens::{LfsGrant, TokenSubject};
use crate::uploads::{ReceivedUpload, receive};
use crate::{DomainEvent, RepoContext, ServiceContext, ServiceError, releases::human_bytes, repositories::viewer_grants};

/// How long an object no repository links any more is kept before maintenance removes it
/// (a client that was told an object exists has this long to fetch it).
pub const ORPHAN_GRACE: Duration = Duration::from_hours(24);
/// Most objects one batch request may name.
pub const MAX_BATCH_OBJECTS: usize = 1000;
/// How long [`LfsService::shutdown`] waits for background work.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
/// Interrupted uploads older than this are swept at start-up.
const STALE_UPLOAD_AGE: Duration = Duration::from_hours(24);
/// Orphans removed per database round trip.
const PRUNE_BATCH: u32 = 500;

/// One object of a batch request, exactly as the client sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchRequestObject {
    pub oid: String,
    pub size: i64,
}

/// A signed `Authorization` header value and how long it stays valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedToken {
    /// The whole header value (`Bearer lfs1.…`).
    pub authorization: String,
    pub expires_in: u64,
}

/// What the batch API answers for one object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchOutcome {
    /// The object may be downloaded with this token.
    Download(IssuedToken),
    /// The client must upload (and then verify) the object with this token.
    Upload(IssuedToken),
    /// Uploading is not needed: the repository has the object already.
    Present,
    /// A download of an object this repository does not have (`404`).
    Missing,
    /// A malformed id or size (`422`).
    Invalid(String),
    /// Larger than `[lfs] max_object_bytes` (`413`).
    TooLarge(String),
}

/// One object of a batch response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchObject {
    pub oid: String,
    pub size: i64,
    pub outcome: BatchOutcome,
}

/// An object opened for download.
#[derive(Debug)]
pub struct LfsDownload {
    pub size: u64,
    pub file: tokio::fs::File,
}

/// A repository opened through a token, and the account the token acts for.
#[derive(Debug, Clone)]
pub struct Granted {
    pub ctx: RepoContext,
    pub user: Option<User>,
    pub grant: LfsGrant,
}

#[derive(Debug, Clone)]
pub struct LfsService {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    ctx: ServiceContext,
    key: TokenKey,
    /// Serialises placing an object's bytes with removing an orphan's, so a sweep never
    /// deletes bytes an upload has just relied on.
    objects: Mutex<()>,
    shutdown: CancellationToken,
    tasks: TaskTracker,
    started: AtomicBool,
}

impl LfsService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self {
            inner: Arc::new(Inner {
                ctx,
                key: TokenKey::random(),
                objects: Mutex::new(()),
                shutdown: CancellationToken::new(),
                tasks: TaskTracker::new(),
                started: AtomicBool::new(false),
            }),
        }
    }

    fn db(&self) -> LfsStore {
        self.inner.ctx.database.lfs()
    }

    /// Whether `[lfs] enabled` is set; everything LFS answers "not found" otherwise.
    pub fn enabled(&self) -> bool {
        self.inner.ctx.config.lfs.enabled
    }

    /// `[lfs] max_object_bytes`.
    pub fn max_object_bytes(&self) -> u64 {
        self.inner.ctx.config.lfs.max_object_bytes
    }

    /// `<lfs_dir>/<aa>/<bb>/<oid>`.
    fn object_path(&self, oid: &LfsOid) -> PathBuf {
        let (first, second) = oid.shards();
        self.inner.ctx.config.storage.lfs_dir().join(first).join(second).join(oid.as_str())
    }

    fn tmp_dir(&self) -> PathBuf {
        self.inner.ctx.config.storage.tmp_dir().join("lfs")
    }

    // ------------------------------------------------------------------ tokens

    /// A signed header for `subject` on `repo`: for `operation` on one object (batch
    /// actions) or on the whole repository (`git-lfs-authenticate`).
    pub fn issue(&self, repo: &RepoContext, subject: TokenSubject, operation: LfsOperation, object: Option<(&LfsOid, u64)>) -> IssuedToken {
        let lifetime = self.inner.ctx.config.lfs.token_lifetime();
        let expires_at = Timestamp(Timestamp::now().seconds().saturating_add(i64::try_from(lifetime.as_secs()).unwrap_or(i64::MAX)));
        let grant = LfsGrant {
            repo: repo.repo.repo.id,
            subject,
            write: operation.is_write(),
            object: object.map(|(oid, size)| (oid.clone(), size)),
        };
        IssuedToken { authorization: format!("Bearer {}", self.inner.key.sign(&grant, expires_at)), expires_in: lifetime.as_secs() }
    }

    /// The grant of a token from [`Self::issue`] (without the `Bearer ` prefix), if it is
    /// genuine and unexpired.
    pub fn verify_token(&self, token: &str) -> Option<LfsGrant> {
        self.inner.key.verify(token, Timestamp::now())
    }

    /// Opens `owner/name` (old names included) for the holder of `grant`. The holder's
    /// access is recomputed now and capped by the grant (read, or read and write), so a
    /// token never outlives the access it was issued for. `NotFound` when the path names
    /// another repository or the holder may no longer read it.
    pub async fn open_granted(&self, grant: LfsGrant, owner: &str, name: &str) -> Result<Granted, ServiceError> {
        let ctx = &self.inner.ctx;
        let name = name.strip_suffix(".git").unwrap_or(name);
        let store = ctx.database.repositories();
        let repo = match store.find_by_path(owner, name).await? {
            Some(repo) => repo,
            None => store.find_by_old_path(owner, name).await?.ok_or(ServiceError::NotFound)?,
        };
        if repo.repo.id != grant.repo {
            return Err(ServiceError::NotFound);
        }
        let (access, user) = match grant.subject {
            TokenSubject::Anonymous => (repo_access(None, &repo, &Grants::default()), None),
            TokenSubject::User(id) => {
                let user = ctx.database.accounts().find_user(id).await?.ok_or(ServiceError::NotFound)?;
                if ctx.database.accounts().is_suspended(id).await? || ctx.database.accounts().password_change_required(id).await? {
                    return Err(ServiceError::Forbidden);
                }
                let grants = viewer_grants(&store, Some(&user), &repo).await?;
                (repo_access(Some(&user), &repo, &grants), Some(user))
            }
            TokenSubject::DeployKey(key) => {
                let keys = ctx.database.ssh_keys().list_deploy_keys(repo.repo.id).await?;
                let access = match keys.iter().find(|k| k.id == key) {
                    Some(key) if key.read_only => RepoAccess::Read,
                    Some(_) => RepoAccess::Write,
                    None => RepoAccess::None,
                };
                (access, None)
            }
        };
        let access = access.min(if grant.write { RepoAccess::Write } else { RepoAccess::Read });
        if !access.can_read() {
            return Err(ServiceError::NotFound);
        }
        Ok(Granted { ctx: self.context(repo, access), user, grant })
    }

    fn context(&self, repo: OwnedRepository, access: RepoAccess) -> RepoContext {
        let storage = &self.inner.ctx.repositories;
        let disk_path = storage.path_for(repo.repo.owner_id, repo.repo.id);
        let reader = storage.reader(repo.repo.owner_id, repo.repo.id);
        // The access is capped by the token, so it is not the user's own.
        RepoContext { repo, access, viewer: None, collaborator: None, reader, disk_path }
    }

    // ------------------------------------------------------------------ batch

    /// The batch API's decision for every object in `objects` (see the module docs for
    /// which objects a repository has). Uploads need write access ([`ServiceError::Forbidden`]
    /// otherwise); refusing mirrors is the caller's business, as for Git pushes. Each
    /// transfer action carries a token for `subject` scoped to that object.
    pub async fn batch(
        &self,
        repo: &RepoContext,
        viewer: Option<&User>,
        subject: TokenSubject,
        operation: LfsOperation,
        objects: &[BatchRequestObject],
    ) -> Result<Vec<BatchObject>, ServiceError> {
        if operation.is_write() {
            if !repo.access.can_write() {
                return Err(ServiceError::Forbidden);
            }
            repo.ensure_writable()?;
        }
        if objects.len() > MAX_BATCH_OBJECTS {
            return Err(ServiceError::Invalid(format!("a batch can name at most {MAX_BATCH_OBJECTS} objects")));
        }
        let valid: Vec<LfsOid> = objects.iter().filter_map(|object| LfsOid::parse(&object.oid).ok()).collect();
        let mut linked = self.db().linked_objects(repo.repo.repo.id, &valid).await?;
        let mut network = None;
        let max = self.max_object_bytes();

        let mut answers = Vec::with_capacity(objects.len());
        for object in objects {
            let mut response_size = object.size;
            let outcome = match (LfsOid::parse(&object.oid), u64::try_from(object.size)) {
                (Err(error), _) => BatchOutcome::Invalid(error.to_string()),
                (_, Err(_)) => BatchOutcome::Invalid("the size cannot be negative".to_owned()),
                (Ok(oid), Ok(size)) => {
                    if !linked.contains_key(&oid)
                        && let Some(found) = self.through_network(repo, viewer, &oid, &mut network).await?
                    {
                        linked.insert(oid.clone(), found);
                    }
                    match (operation, linked.get(&oid).copied()) {
                        (LfsOperation::Download, Some(stored)) => {
                            response_size = i64::try_from(stored).map_err(ServiceError::internal)?;
                            BatchOutcome::Download(self.issue(repo, subject, operation, Some((&oid, stored))))
                        }
                        (LfsOperation::Download, None) => BatchOutcome::Missing,
                        (LfsOperation::Upload, Some(stored)) if stored == size && self.bytes_present(&oid).await => BatchOutcome::Present,
                        (LfsOperation::Upload, _) if size > max => {
                            BatchOutcome::TooLarge(format!("objects can be at most {}", human_bytes(max)))
                        }
                        (LfsOperation::Upload, _) => BatchOutcome::Upload(self.issue(repo, subject, operation, Some((&oid, size)))),
                    }
                }
            };
            answers.push(BatchObject { oid: object.oid.clone(), size: response_size, outcome });
        }
        Ok(answers)
    }

    /// Whether the bytes of `oid` are on disk (a row without bytes needs a new upload).
    async fn bytes_present(&self, oid: &LfsOid) -> bool {
        tokio::fs::try_exists(self.object_path(oid)).await.unwrap_or(false)
    }

    /// The size of `oid` if another repository of `repo`'s fork network that `viewer`
    /// may read has it, after linking it to `repo` as well. `network` caches the
    /// readable network across the objects of one batch.
    async fn through_network(
        &self,
        repo: &RepoContext,
        viewer: Option<&User>,
        oid: &LfsOid,
        network: &mut Option<Vec<RepoId>>,
    ) -> Result<Option<u64>, ServiceError> {
        let id = repo.repo.repo.id;
        if network.is_none() {
            let readable =
                self.inner.ctx.database.forks().network_readable(id, viewer.map(User::id), viewer.is_some_and(|v| v.is_admin)).await?;
            *network = Some(readable.into_iter().map(|r| r.repo.id).filter(|other| *other != id).collect());
        }
        let others = network.as_deref().unwrap_or_default();
        if self.db().linked_in_any(others, oid).await?.is_none() || !self.db().link(id, oid).await? {
            return Ok(None);
        }
        Ok(self.db().find_object(oid).await?.map(|object| object.size))
    }

    /// The size of `oid` if `repo` may use it (linked, or through the fork network), for
    /// the blob page.
    pub async fn find(&self, repo: &RepoContext, viewer: Option<&User>, oid: &LfsOid) -> Result<Option<u64>, ServiceError> {
        if let Some(size) = self.db().linked_objects(repo.repo.repo.id, std::slice::from_ref(oid)).await?.remove(oid) {
            return Ok(Some(size));
        }
        self.through_network(repo, viewer, oid, &mut None).await
    }

    // ------------------------------------------------------------------ transfers

    /// Stores `body` as object `oid` of `size` bytes for `repo` and links it (see the
    /// module docs for the procedure). This is the service API for anything that brings
    /// objects into a repository (uploads, importers). Needs write access; fails with
    /// [`ServiceError::Invalid`] when the bytes do not match `oid` and `size`, or `size`
    /// exceeds `[lfs] max_object_bytes`.
    pub async fn store(
        &self,
        repo: &RepoContext,
        oid: &LfsOid,
        size: u64,
        mut body: impl AsyncRead + Unpin + Send,
    ) -> Result<(), ServiceError> {
        if !repo.access.can_write() {
            return Err(ServiceError::Forbidden);
        }
        repo.ensure_writable()?;
        let max = self.max_object_bytes();
        if size > max {
            return Err(ServiceError::Invalid(format!("objects can be at most {}", human_bytes(max))));
        }
        let tmp_dir = self.tmp_dir();
        tokio::fs::create_dir_all(&tmp_dir).await.map_err(ServiceError::internal)?;
        let tmp = tmp_dir.join(format!("{}.part", random_hex()?));
        let received = match receive(&mut body, &tmp, size).await {
            Ok(received) => received,
            Err(error) => {
                remove_quietly(&tmp).await;
                return Err(ServiceError::internal(error));
            }
        };
        let refusal = match received.as_ref() {
            None => Some(format!("the upload is larger than the announced {size} bytes")),
            Some(upload) if upload.size != size => Some(format!("received {} bytes, but the object has {size}", upload.size)),
            Some(upload) if LfsOid::from_digest(upload.digest) != *oid => Some("the content does not match the object id".to_owned()),
            Some(_) => None,
        };
        if let Some(refusal) = refusal {
            remove_quietly(&tmp).await;
            return Err(ServiceError::Invalid(refusal));
        }
        let received = received.expect("validated upload is present");
        let service = self.clone();
        let repo_id = repo.repo.repo.id;
        let owned_oid = oid.clone();
        // Complete placement and its database transaction even if the request ends.
        // Otherwise cancellation after rename would leave an object outside pruning.
        self.inner
            .tasks
            .spawn(async move { service.place_object(repo_id, &owned_oid, received, tmp).await })
            .await
            .map_err(ServiceError::internal)??;
        tracing::info!(repo = %repo.repo.full_name(), %oid, size, "LFS object stored");
        Ok(())
    }

    async fn place_object(&self, repo: RepoId, oid: &LfsOid, mut received: ReceivedUpload, tmp: PathBuf) -> Result<(), ServiceError> {
        let _placing = self.inner.objects.lock().await;
        let target = self.object_path(oid);
        if self.bytes_present(oid).await {
            remove_quietly(&tmp).await;
        } else {
            let placed = async {
                if let Some(parent) = target.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::rename(&tmp, &target).await
            };
            if let Err(error) = placed.await {
                tracing::error!(%error, path = %target.display(), "cannot move an uploaded LFS object into place; discarding it");
                remove_quietly(&tmp).await;
                return Err(ServiceError::internal(error));
            }
        }
        // The target may already be shared, and a database error does not establish
        // whether COMMIT took effect. Never remove its bytes on an uncertain outcome.
        self.db().record_object(oid, received.size, repo).await?;
        received.placed();
        Ok(())
    }

    /// The basic transfer's verify step: `oid` is linked to `repo` with `size` bytes.
    pub async fn verify(&self, repo: &RepoContext, oid: &LfsOid, size: u64) -> Result<(), ServiceError> {
        match self.db().linked_objects(repo.repo.repo.id, std::slice::from_ref(oid)).await?.get(oid) {
            Some(stored) if *stored == size => Ok(()),
            Some(stored) => Err(ServiceError::Invalid(format!("the object has {stored} bytes, not {size}"))),
            None => Err(ServiceError::NotFound),
        }
    }

    /// Opens object `oid` for download if `repo` may use it.
    pub async fn open(&self, repo: &RepoContext, viewer: Option<&User>, oid: &LfsOid) -> Result<LfsDownload, ServiceError> {
        let size = self.find(repo, viewer, oid).await?.ok_or(ServiceError::NotFound)?;
        match tokio::fs::File::open(self.object_path(oid)).await {
            Ok(file) => Ok(LfsDownload { size, file }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                tracing::error!(%oid, "the bytes of an LFS object are missing");
                Err(ServiceError::NotFound)
            }
            Err(error) => Err(ServiceError::internal(error)),
        }
    }

    /// How many objects `repo` uses and their total size.
    pub async fn usage(&self, repo: &RepoContext) -> Result<(u64, u64), ServiceError> {
        Ok(self.db().usage(repo.repo.repo.id).await?)
    }

    // ------------------------------------------------------------------ background

    /// Starts the fork subscriber and the orphan clean-up schedule and sweeps interrupted
    /// uploads. Called once at start-up; later calls do nothing.
    pub fn spawn_background(&self) {
        if self.inner.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let events = self.inner.ctx.events.subscribe();
        let service = self.clone();
        self.inner.tasks.spawn(async move { service.listen(events).await });
        let sweeper = self.clone();
        self.inner.tasks.spawn(async move {
            let swept = sweep_stale_uploads(&sweeper.tmp_dir(), STALE_UPLOAD_AGE).await;
            if swept > 0 {
                tracing::info!(swept, "removed interrupted LFS uploads");
            }
        });
        if let Some(interval) = self.inner.ctx.config.operations.maintenance_interval() {
            let service = self.clone();
            self.inner.tasks.spawn(async move { service.prune_periodically(interval).await });
        }
    }

    /// Stops background work and waits (bounded) for it to finish. Called from
    /// [`crate::Services::shutdown`] before the stores close.
    pub async fn shutdown(&self) {
        self.inner.shutdown.cancel();
        self.inner.tasks.close();
        if tokio::time::timeout(SHUTDOWN_GRACE, self.inner.tasks.wait()).await.is_err() {
            tracing::warn!("LFS background work did not stop within {SHUTDOWN_GRACE:?}");
        }
    }

    async fn listen(self, mut events: Receiver<DomainEvent>) {
        loop {
            let event = tokio::select! {
                () = self.inner.shutdown.cancelled() => break,
                event = events.recv() => event,
            };
            match event {
                Ok(DomainEvent::RepositoryForked { repo, parent, .. }) => match self.db().copy_links(parent, repo).await {
                    Ok(copied) if copied > 0 => {
                        tracing::info!(repo = repo.0, parent = parent.0, copied, "LFS objects linked to a new fork");
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, repo = repo.0, "cannot link LFS objects to a new fork"),
                },
                Ok(_) => {}
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "LFS subscriber fell behind; new forks reach their parent's objects through the network");
                }
                Err(RecvError::Closed) => break,
            }
        }
    }

    async fn prune_periodically(self, interval: Duration) {
        loop {
            tokio::select! {
                () = self.inner.shutdown.cancelled() => break,
                () = tokio::time::sleep(interval) => {}
            }
            match self.prune_orphans(ORPHAN_GRACE).await {
                Ok(0) => {}
                Ok(removed) => tracing::info!(removed, "removed LFS objects no repository uses"),
                Err(error) => tracing::warn!(%error, "removing unused LFS objects failed"),
            }
        }
    }

    /// Removes the objects (rows, then bytes) that no repository has linked for `grace`;
    /// returns how many. Part of maintenance, also callable directly.
    pub async fn prune_orphans(&self, grace: Duration) -> Result<u64, ServiceError> {
        let cutoff = Timestamp(Timestamp::now().seconds().saturating_sub(i64::try_from(grace.as_secs()).unwrap_or(i64::MAX)));
        let mut removed = 0;
        loop {
            let orphans = self.db().orphans(cutoff, PRUNE_BATCH).await?;
            if orphans.is_empty() || self.inner.shutdown.is_cancelled() {
                return Ok(removed);
            }
            let _placing = self.inner.objects.lock().await;
            for oid in orphans {
                if !self.db().delete_orphan(&oid, cutoff).await? {
                    continue;
                }
                removed += 1;
                let path = self.object_path(&oid);
                if let Err(error) = tokio::fs::remove_file(&path).await
                    && error.kind() != io::ErrorKind::NotFound
                {
                    tracing::warn!(%error, path = %path.display(), "cannot remove an unused LFS object");
                }
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn completed_upload_finishes_after_its_request_is_cancelled() {
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
        let oid = LfsOid::parse("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824").unwrap();
        let lfs = &services.lfs;
        let placing = lfs.inner.objects.lock().await;
        let service = lfs.clone();
        let target = oid.clone();
        let upload_repo = repo.clone();
        let request = tokio::spawn(async move { service.store(&upload_repo, &target, 5, b"hello".as_slice()).await });
        tokio::time::timeout(Duration::from_secs(5), async {
            while lfs.inner.tasks.is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();

        request.abort();
        assert!(request.await.unwrap_err().is_cancelled());
        drop(placing);
        lfs.shutdown().await;
        lfs.verify(&repo, &oid, 5).await.unwrap();
        assert_eq!(tokio::fs::read(lfs.object_path(&oid)).await.unwrap(), b"hello");
        assert_eq!(std::fs::read_dir(lfs.tmp_dir()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn uploads_stop_at_the_announced_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.part");
        let mut body: &[u8] = b"hello world";
        assert!(receive(&mut body, &path, 5).await.unwrap().is_none());
        let mut body: &[u8] = b"hello";
        let upload = receive(&mut body, &dir.path().join("b.part"), 5).await.unwrap().unwrap();
        assert_eq!(upload.size, 5);
        assert_eq!(LfsOid::from_digest(upload.digest).as_str(), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[tokio::test]
    async fn stale_uploads_are_swept() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.part"), "x").unwrap();
        std::fs::write(dir.path().join("keep.txt"), "x").unwrap();
        assert_eq!(sweep_stale_uploads(dir.path(), Duration::from_secs(3600)).await, 0);
        assert_eq!(sweep_stale_uploads(dir.path(), Duration::ZERO).await, 1);
        assert!(dir.path().join("keep.txt").exists());
    }
}
