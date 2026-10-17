//! Git LFS objects, batch decisions, tokens and locks against real stores and a
//! temporary data directory.

use std::{sync::Arc, time::Duration};

use rbit_core::{
    Config, Permission, User, Visibility,
    lfs::{LfsOid, LfsOperation},
};
use rbit_services::{
    CreateRepository, RepoContext, ServiceError, Services,
    forks::ForkInput,
    lfs::{
        BatchOutcome, BatchRequestObject, TokenSubject,
        locks::{LockCreated, LockQuery},
    },
};
use rbit_storage::{Database, NewUser};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
    config: Arc<Config>,
}

impl Fixture {
    async fn new(configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        configure(&mut config);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = rbit_storage::KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        Self { _dir: dir, database, services, config }
    }

    async fn user(&self, name: &str, is_admin: bool) -> User {
        self.database
            .accounts()
            .create_user(NewUser {
                name: name.parse().unwrap(),
                email: format!("{name}@example.com").parse().unwrap(),
                display_name: None,
                password_hash: "not-a-real-hash".to_owned(),
                is_admin,
            })
            .await
            .unwrap()
    }

    async fn repo(&self, owner: &User, name: &str, visibility: Visibility) -> RepoContext {
        let input = CreateRepository { name: name.to_owned(), description: String::new(), visibility };
        self.services.repositories.create(owner, input).await.unwrap();
        self.open(Some(owner), owner.name().as_str(), name).await.unwrap()
    }

    async fn open(&self, viewer: Option<&User>, owner: &str, name: &str) -> Result<RepoContext, ServiceError> {
        self.services.repositories.open(viewer, owner, name).await
    }

    fn object_path(&self, oid: &LfsOid) -> std::path::PathBuf {
        let (a, b) = oid.shards();
        self.config.storage.lfs_dir().join(a).join(b).join(oid.as_str())
    }

    fn object_files(&self) -> usize {
        let Ok(first) = std::fs::read_dir(self.config.storage.lfs_dir()) else { return 0 };
        first
            .flat_map(|a| std::fs::read_dir(a.unwrap().path()).unwrap())
            .flat_map(|b| std::fs::read_dir(b.unwrap().path()).unwrap())
            .count()
    }
}

fn oid_of(content: &[u8]) -> LfsOid {
    LfsOid::from_digest(Sha256::digest(content).into())
}

fn request(content: &[u8]) -> BatchRequestObject {
    BatchRequestObject { oid: oid_of(content).to_string(), size: i64::try_from(content.len()).unwrap() }
}

async fn store(services: &Services, repo: &RepoContext, content: &[u8]) -> Result<(), ServiceError> {
    services.lfs.store(repo, &oid_of(content), content.len() as u64, content).await
}

#[tokio::test]
async fn uploads_are_verified_stored_once_and_downloaded() {
    let fixture = Fixture::new(|config| config.lfs.max_object_bytes = 64).await;
    let alice = fixture.user("alice", false).await;
    let repo = fixture.repo(&alice, "assets", Visibility::Public).await;
    let lfs = &fixture.services.lfs;
    let content = b"a large binary file".as_slice();
    let oid = oid_of(content);

    // Wrong size, wrong content, too large: nothing is stored.
    let short = lfs.store(&repo, &oid, content.len() as u64 + 1, content).await;
    assert!(matches!(short, Err(ServiceError::Invalid(ref m)) if m.contains("received")), "{short:?}");
    let long = lfs.store(&repo, &oid, 3, content).await;
    assert!(matches!(long, Err(ServiceError::Invalid(ref m)) if m.contains("larger than the announced")), "{long:?}");
    let other = lfs.store(&repo, &oid_of(b"something else entirely"), content.len() as u64, content).await;
    assert!(matches!(other, Err(ServiceError::Invalid(ref m)) if m.contains("does not match")), "{other:?}");
    let huge = lfs.store(&repo, &oid, 65, content).await;
    assert!(matches!(huge, Err(ServiceError::Invalid(ref m)) if m.contains("at most 64 bytes")), "{huge:?}");
    assert!(matches!(lfs.verify(&repo, &oid, content.len() as u64).await, Err(ServiceError::NotFound)));
    assert_eq!(fixture.object_files(), 0);
    let tmp = fixture.config.storage.tmp_dir().join("lfs");
    assert_eq!(std::fs::read_dir(&tmp).unwrap().count(), 0, "refused uploads leave no temporary files");

    store(&fixture.services, &repo, content).await.unwrap();
    store(&fixture.services, &repo, content).await.unwrap();
    assert_eq!(fixture.object_files(), 1);
    assert!(fixture.object_path(&oid).is_file());
    lfs.verify(&repo, &oid, content.len() as u64).await.unwrap();
    assert!(matches!(lfs.verify(&repo, &oid, 1).await, Err(ServiceError::Invalid(_))));
    assert_eq!(lfs.usage(&repo).await.unwrap(), (1, content.len() as u64));

    let mut download = lfs.open(&repo, Some(&alice), &oid).await.unwrap();
    assert_eq!(download.size, content.len() as u64);
    let mut bytes = Vec::new();
    download.file.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes, content);

    // Readers cannot upload.
    let bob = fixture.user("bob", false).await;
    let as_bob = fixture.open(Some(&bob), "alice", "assets").await.unwrap();
    assert!(matches!(store(&fixture.services, &as_bob, b"x").await, Err(ServiceError::Forbidden)));
}

#[tokio::test]
async fn the_batch_api_decides_per_object() {
    let fixture = Fixture::new(|config| config.lfs.max_object_bytes = 100).await;
    let alice = fixture.user("alice", false).await;
    let repo = fixture.repo(&alice, "assets", Visibility::Public).await;
    let lfs = &fixture.services.lfs;
    store(&fixture.services, &repo, b"stored").await.unwrap();
    let subject = TokenSubject::User(alice.id());

    let objects = [
        request(b"stored"),
        request(b"missing"),
        BatchRequestObject { oid: "not-an-oid".into(), size: 1 },
        BatchRequestObject { oid: oid_of(b"x").to_string(), size: -1 },
        BatchRequestObject { oid: oid_of(b"big").to_string(), size: 101 },
    ];
    let download = lfs.batch(&repo, Some(&alice), subject, LfsOperation::Download, &objects).await.unwrap();
    assert!(matches!(download[0].outcome, BatchOutcome::Download(ref token) if token.expires_in == 3600));
    assert_eq!(download[1].outcome, BatchOutcome::Missing);
    assert!(matches!(download[2].outcome, BatchOutcome::Invalid(_)));
    assert!(matches!(download[3].outcome, BatchOutcome::Invalid(_)));
    assert_eq!(download[4].outcome, BatchOutcome::Missing);
    assert_eq!(download[2].oid, "not-an-oid", "objects are echoed as sent");

    let upload = lfs.batch(&repo, Some(&alice), subject, LfsOperation::Upload, &objects).await.unwrap();
    assert_eq!(upload[0].outcome, BatchOutcome::Present);
    assert!(matches!(upload[1].outcome, BatchOutcome::Upload(_)));
    assert!(matches!(upload[4].outcome, BatchOutcome::TooLarge(ref m) if m.contains("100 bytes")));

    // The upload token is scoped to the object and its size.
    let BatchOutcome::Upload(token) = &upload[1].outcome else { unreachable!() };
    let grant = lfs.verify_token(token.authorization.strip_prefix("Bearer ").unwrap()).unwrap();
    assert_eq!(grant.object, Some((oid_of(b"missing"), 7)));
    assert!(grant.write);

    // Bytes missing on disk: the client is asked to upload again.
    std::fs::remove_file(fixture.object_path(&oid_of(b"stored"))).unwrap();
    let again = lfs.batch(&repo, Some(&alice), subject, LfsOperation::Upload, &objects[..1]).await.unwrap();
    assert!(matches!(again[0].outcome, BatchOutcome::Upload(_)));

    let bob = fixture.user("bob", false).await;
    let as_bob = fixture.open(Some(&bob), "alice", "assets").await.unwrap();
    let refused = lfs.batch(&as_bob, Some(&bob), TokenSubject::User(bob.id()), LfsOperation::Upload, &objects).await;
    assert!(matches!(refused, Err(ServiceError::Forbidden)));
    let too_many = vec![request(b"x"); 1001];
    assert!(matches!(lfs.batch(&repo, Some(&alice), subject, LfsOperation::Download, &too_many).await, Err(ServiceError::Invalid(_))));
}

#[tokio::test]
async fn knowing_an_object_id_never_grants_its_content() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice", false).await;
    let mallory = fixture.user("mallory", false).await;
    let secret_repo = fixture.repo(&alice, "secret", Visibility::Private).await;
    let own_repo = fixture.repo(&mallory, "mine", Visibility::Public).await;
    let lfs = &fixture.services.lfs;
    let secret = b"top secret design".as_slice();
    store(&fixture.services, &secret_repo, secret).await.unwrap();

    let subject = TokenSubject::User(mallory.id());
    let download = lfs.batch(&own_repo, Some(&mallory), subject, LfsOperation::Download, &[request(secret)]).await.unwrap();
    assert_eq!(download[0].outcome, BatchOutcome::Missing);
    let upload = lfs.batch(&own_repo, Some(&mallory), subject, LfsOperation::Upload, &[request(secret)]).await.unwrap();
    assert!(matches!(upload[0].outcome, BatchOutcome::Upload(_)), "the content must be uploaded, not merely named");
    assert!(matches!(lfs.open(&own_repo, Some(&mallory), &oid_of(secret)).await, Err(ServiceError::NotFound)));

    // Uploading the same content links the stored object: still one file.
    store(&fixture.services, &own_repo, secret).await.unwrap();
    assert_eq!(fixture.object_files(), 1);
    lfs.open(&own_repo, None, &oid_of(secret)).await.unwrap();
}

#[tokio::test]
async fn forks_reach_their_network_s_objects() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let parent = fixture.repo(&alice, "assets", Visibility::Public).await;
    let lfs = &fixture.services.lfs;
    store(&fixture.services, &parent, b"before the fork").await.unwrap();

    lfs.spawn_background();
    let as_bob = fixture.open(Some(&bob), "alice", "assets").await.unwrap();
    fixture.services.forks.fork(&bob, &as_bob, ForkInput { owner: String::new(), name: String::new() }).await.unwrap();
    let fork = fixture.open(Some(&bob), "bob", "assets").await.unwrap();

    // The parent's links are copied in the background.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while lfs.usage(&fork).await.unwrap().0 == 0 {
        assert!(tokio::time::Instant::now() < deadline, "links were not copied to the fork");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Objects added to the parent later are found through the network, then linked.
    store(&fixture.services, &parent, b"after the fork").await.unwrap();
    let subject = TokenSubject::User(bob.id());
    let answer = lfs.batch(&fork, Some(&bob), subject, LfsOperation::Download, &[request(b"after the fork")]).await.unwrap();
    assert!(matches!(answer[0].outcome, BatchOutcome::Download(_)));
    assert_eq!(lfs.usage(&fork).await.unwrap().0, 2);
    let upload = lfs.batch(&fork, Some(&bob), subject, LfsOperation::Upload, &[request(b"after the fork")]).await.unwrap();
    assert_eq!(upload[0].outcome, BatchOutcome::Present, "no second upload within the network");
    assert_eq!(lfs.find(&parent, Some(&alice), &oid_of(b"after the fork")).await.unwrap(), Some(14));
    lfs.shutdown().await;
}

#[tokio::test]
async fn tokens_open_the_repository_with_current_access() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let repo = fixture.repo(&alice, "assets", Visibility::Private).await;
    let lfs = &fixture.services.lfs;
    fixture.services.collaborators.add(&alice, &repo, "bob", Permission::Write).await.unwrap();
    let as_bob = fixture.open(Some(&bob), "alice", "assets").await.unwrap();

    let token = |operation| {
        let issued = lfs.issue(&as_bob, TokenSubject::User(bob.id()), operation, None);
        lfs.verify_token(issued.authorization.strip_prefix("Bearer ").unwrap()).unwrap()
    };
    let granted = lfs.open_granted(token(LfsOperation::Upload), "alice", "assets.git").await.unwrap();
    assert!(granted.ctx.access.can_write());
    assert_eq!(granted.user.as_ref().map(User::id), Some(bob.id()));
    let read_only = lfs.open_granted(token(LfsOperation::Download), "alice", "assets").await.unwrap();
    assert!(read_only.ctx.access.can_read() && !read_only.ctx.access.can_write(), "a download token never writes");

    let other = fixture.repo(&alice, "other", Visibility::Public).await;
    assert!(matches!(lfs.open_granted(token(LfsOperation::Upload), "alice", "other").await, Err(ServiceError::NotFound)));
    drop(other);

    // Removing the collaborator ends the token's access at once.
    fixture.services.collaborators.remove(&alice, &repo, "bob").await.unwrap();
    assert!(matches!(lfs.open_granted(token(LfsOperation::Download), "alice", "assets").await, Err(ServiceError::NotFound)));

    // Anonymous tokens read public repositories only.
    let public = fixture.repo(&alice, "public", Visibility::Public).await;
    let issued = lfs.issue(&public, TokenSubject::Anonymous, LfsOperation::Upload, None);
    let grant = lfs.verify_token(issued.authorization.strip_prefix("Bearer ").unwrap()).unwrap();
    let anonymous = lfs.open_granted(grant, "alice", "public").await.unwrap();
    assert!(!anonymous.ctx.access.can_write() && anonymous.user.is_none());

    // Suspended users are refused.
    let granted = lfs.issue(&repo, TokenSubject::User(alice.id()), LfsOperation::Download, None);
    let grant = lfs.verify_token(granted.authorization.strip_prefix("Bearer ").unwrap()).unwrap();
    fixture.database.admin().set_suspended(alice.id(), true).await.unwrap();
    assert!(matches!(lfs.open_granted(grant, "alice", "assets").await, Err(ServiceError::Forbidden)));
    assert_eq!(lfs.verify_token("lfs1.forged.token"), None);
}

#[tokio::test]
async fn locks_belong_to_writers_and_admins_may_force() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let carol = fixture.user("carol", false).await;
    let repo = fixture.repo(&alice, "assets", Visibility::Public).await;
    fixture.services.collaborators.add(&alice, &repo, "bob", Permission::Write).await.unwrap();
    let as_bob = fixture.open(Some(&bob), "alice", "assets").await.unwrap();
    let as_carol = fixture.open(Some(&carol), "alice", "assets").await.unwrap();
    let lfs = &fixture.services.lfs;

    let LockCreated::Created(logo) = lfs.create_lock(&as_bob, Some(&bob), "logo.psd").await.unwrap() else { panic!("created") };
    assert_eq!(logo.owner.name, "bob");
    assert_eq!(lfs.create_lock(&repo, Some(&alice), "logo.psd").await.unwrap(), LockCreated::Exists(logo.clone()));
    let LockCreated::Created(song) = lfs.create_lock(&repo, Some(&alice), "song.wav").await.unwrap() else { panic!("created") };
    assert!(matches!(lfs.create_lock(&as_carol, Some(&carol), "x").await, Err(ServiceError::Forbidden)), "readers cannot lock");
    assert!(matches!(lfs.create_lock(&repo, None, "x").await, Err(ServiceError::Forbidden)), "locks need an account");
    assert!(matches!(lfs.create_lock(&repo, Some(&alice), "").await, Err(ServiceError::Invalid(_))));

    // Readers list; pages follow the cursor.
    let first = lfs.list_locks(&as_carol, &LockQuery { limit: Some(1), ..LockQuery::default() }).await.unwrap();
    assert_eq!(first.locks, std::slice::from_ref(&logo));
    let cursor = first.next_cursor.clone().unwrap();
    let second = lfs.list_locks(&as_carol, &LockQuery { cursor: Some(cursor), limit: Some(1), ..LockQuery::default() }).await.unwrap();
    assert_eq!((second.locks.len(), second.next_cursor), (1, None));
    let by_path = lfs.list_locks(&as_carol, &LockQuery { path: Some("song.wav".into()), ..LockQuery::default() }).await.unwrap();
    assert_eq!(by_path.locks, std::slice::from_ref(&song));
    let bad_id = lfs.list_locks(&as_carol, &LockQuery { id: Some("abc".into()), ..LockQuery::default() }).await.unwrap();
    assert!(bad_id.locks.is_empty(), "a malformed id matches nothing");

    let verified = lfs.verify_locks(&as_bob, Some(&bob), None, None).await.unwrap();
    assert_eq!((verified.ours, verified.theirs), (vec![logo.clone()], vec![song.clone()]));
    assert!(matches!(lfs.verify_locks(&as_carol, Some(&carol), None, None).await, Err(ServiceError::Forbidden)));

    // Only the owner unlocks, unless an administrator forces it.
    let id = song.id.0.to_string();
    assert!(matches!(lfs.unlock(&as_bob, Some(&bob), &id, false).await, Err(ServiceError::Forbidden)));
    assert!(matches!(lfs.unlock(&as_bob, Some(&bob), &id, true).await, Err(ServiceError::Forbidden)), "force is for administrators");
    assert_eq!(lfs.unlock(&repo, Some(&alice), &logo.id.0.to_string(), true).await.unwrap(), logo);
    assert_eq!(lfs.unlock(&repo, Some(&alice), &id, false).await.unwrap(), song);
    assert!(matches!(lfs.unlock(&repo, Some(&alice), &id, false).await, Err(ServiceError::NotFound)));
    assert!(matches!(lfs.unlock(&repo, Some(&alice), "nope", false).await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn deleting_a_repository_leaves_orphans_that_maintenance_removes() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice", false).await;
    let doomed = fixture.repo(&alice, "doomed", Visibility::Public).await;
    let kept = fixture.repo(&alice, "kept", Visibility::Public).await;
    let lfs = &fixture.services.lfs;
    store(&fixture.services, &doomed, b"only in doomed").await.unwrap();
    store(&fixture.services, &doomed, b"shared").await.unwrap();
    store(&fixture.services, &kept, b"shared").await.unwrap();

    fixture.services.repo_settings.delete(&alice, &doomed, "alice/doomed").await.unwrap();
    assert_eq!(lfs.prune_orphans(Duration::from_secs(3600)).await.unwrap(), 0, "within the grace period");
    assert!(fixture.object_path(&oid_of(b"only in doomed")).is_file());

    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(lfs.prune_orphans(Duration::ZERO).await.unwrap(), 1);
    assert!(!fixture.object_path(&oid_of(b"only in doomed")).exists());
    assert!(fixture.object_path(&oid_of(b"shared")).is_file(), "still used by another repository");
    lfs.verify(&kept, &oid_of(b"shared"), 6).await.unwrap();
}
