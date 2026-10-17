//! Releases and their attached files against real stores, bare repositories and a
//! temporary attachments directory.

use std::{
    path::Path,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

use rbit_core::{Config, User, Visibility};
use rbit_services::{CreateRepository, DomainEvent, RepoContext, ServiceError, Services, releases::ReleaseInput};
use rbit_storage::{Database, KvStore, NewUser};
use tokio::io::AsyncReadExt;

struct Fixture {
    dir: tempfile::TempDir,
    database: Database,
    services: Services,
    config: Arc<Config>,
}

impl Fixture {
    async fn new(configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        configure(&mut config);
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        Self { dir, database, services, config }
    }

    async fn user(&self, name: &str) -> User {
        self.database
            .accounts()
            .create_user(NewUser {
                name: name.parse().unwrap(),
                email: format!("{name}@example.com").parse().unwrap(),
                display_name: None,
                password_hash: "not-a-real-hash".to_owned(),
                is_admin: false,
            })
            .await
            .unwrap()
    }

    /// A public repository of `owner` with `main` and the tags `v1` and `v2`.
    async fn repo(&self, owner: &User) -> RepoContext {
        let input = CreateRepository { name: "demo".to_owned(), description: String::new(), visibility: Visibility::Public };
        self.services.repositories.create(owner, input).await.unwrap();
        let repo = self.open(Some(owner)).await.unwrap();
        let work = self.dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("README.md"), "hello\n").unwrap();
        git(&work, &["init", "--quiet", "--initial-branch=main"]);
        git(&work, &["add", "README.md"]);
        git(&work, &["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--quiet", "-m", "initial"]);
        git(&work, &["tag", "v1"]);
        git(&work, &["tag", "v2"]);
        git(&work, &["push", "--quiet", repo.disk_path.to_str().unwrap(), "main", "v1", "v2"]);
        repo
    }

    async fn open(&self, viewer: Option<&User>) -> Result<RepoContext, ServiceError> {
        self.services.repositories.open(viewer, "alice", "demo").await
    }
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn input(tag: &str, draft: bool) -> ReleaseInput {
    ReleaseInput {
        tag_name: tag.to_owned(),
        name: String::new(),
        notes: "Notes".to_owned(),
        is_draft: draft,
        is_prerelease: false,
        target: String::new(),
    }
}

#[tokio::test]
async fn releases_need_an_existing_unused_tag_and_writers() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let repo = fixture.repo(&alice).await;
    let releases = &fixture.services.releases;

    let created = releases.create(&repo, &alice, input("v1", false)).await.unwrap();
    assert_eq!(created.name, "v1", "the title defaults to the tag");
    assert!(matches!(releases.create(&repo, &alice, input("v1", false)).await, Err(ServiceError::Conflict(_))));
    assert!(matches!(releases.create(&repo, &alice, input("v9", false)).await, Err(ServiceError::Invalid(_))));
    assert!(matches!(releases.create(&repo, &alice, input("  ", false)).await, Err(ServiceError::Invalid(_))));

    let as_bob = fixture.open(Some(&bob)).await.unwrap();
    assert!(matches!(releases.create(&as_bob, &bob, input("v2", false)).await, Err(ServiceError::Forbidden)));
    assert!(matches!(releases.delete(&as_bob, created.id).await, Err(ServiceError::Forbidden)));
}

#[tokio::test]
async fn a_release_can_create_its_tag_from_a_branch() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let repo = fixture.repo(&alice).await;
    let releases = &fixture.services.releases;
    let mut events = fixture.services.events.subscribe();

    let new_tag = |tag: &str, target: &str| ReleaseInput { target: target.to_owned(), ..input(tag, false) };
    let release = releases.create(&repo, &alice, new_tag("v3", "main")).await.unwrap();
    assert_eq!(release.tag_name, "v3");
    let main = repo.reader.commit_id("main".into()).await.unwrap();
    assert_eq!(repo.reader.commit_id("v3".into()).await.unwrap(), main);
    let DomainEvent::RepositoryPushed { updates, pusher, .. } = events.recv().await.unwrap() else {
        panic!("the new tag is reported first")
    };
    assert_eq!((updates.len(), updates[0].tag(), pusher), (1, Some("v3"), Some(alice.id())));
    assert!(matches!(events.recv().await.unwrap(), DomainEvent::ReleasePublished { .. }));

    assert!(matches!(releases.create(&repo, &alice, new_tag("v4", "nope")).await, Err(ServiceError::Invalid(_))));
    assert!(matches!(releases.create(&repo, &alice, new_tag("bad tag", "main")).await, Err(ServiceError::Invalid(_))));
    // An existing tag is used as is; the target is ignored.
    assert_eq!(releases.create(&repo, &alice, new_tag("v1", "nope")).await.unwrap().tag_name, "v1");
}

#[tokio::test]
async fn drafts_are_hidden_from_readers_and_published_once() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let repo = fixture.repo(&alice).await;
    let releases = &fixture.services.releases;
    let mut events = fixture.services.events.subscribe();

    let draft = releases.create(&repo, &alice, input("v2", true)).await.unwrap();
    let anonymous = fixture.open(None).await.unwrap();
    assert!(matches!(releases.by_tag(&anonymous, "v2").await, Err(ServiceError::NotFound)));
    assert!(matches!(releases.by_id(&anonymous, draft.id).await, Err(ServiceError::NotFound)));
    assert_eq!(releases.list(&anonymous, 1).await.unwrap().total, 0);
    assert_eq!(releases.list(&repo, 1).await.unwrap().total, 1, "writers see drafts");
    assert!(releases.latest(&anonymous).await.unwrap().is_none());

    let mut publish = input("v2", false);
    publish.name = "Second".to_owned();
    let published = releases.update(&repo, &alice, draft.id, publish.clone()).await.unwrap();
    assert!(published.published_at.is_some());
    assert_eq!(releases.by_tag(&anonymous, "v2").await.unwrap().release.name, "Second");
    // Editing a published release does not announce it again.
    releases.update(&repo, &alice, draft.id, publish).await.unwrap();

    let event = events.try_recv().unwrap();
    assert_eq!(event, DomainEvent::ReleasePublished { repo: repo.repo.repo.id, release: draft.id, actor: alice.id() });
    assert!(events.try_recv().is_err(), "published exactly once");
}

#[tokio::test]
async fn files_are_streamed_to_disk_and_downloaded() {
    let fixture = Fixture::new(|config| config.releases.max_asset_bytes = 10).await;
    let alice = fixture.user("alice").await;
    let repo = fixture.repo(&alice).await;
    let releases = &fixture.services.releases;
    let release = releases.create(&repo, &alice, input("v1", false)).await.unwrap();

    let asset = releases.upload_asset(&repo, &alice, release.id, "app.zip", "application/zip", &b"0123456789"[..]).await.unwrap();
    assert_eq!(asset.size, 10);
    assert_eq!(asset.sha256_hex(), "84d89877f0d4041efb6bf91a16f0248f2fd573e6af05c19f96bedb9f882f7882");
    let on_disk = fixture.config.storage.attachments_dir().join(repo.repo.repo.id.0.to_string()).join(asset.id.0.to_string());
    assert_eq!(std::fs::read(&on_disk).unwrap(), b"0123456789");

    let too_big = releases.upload_asset(&repo, &alice, release.id, "big.bin", "", &b"0123456789A"[..]).await;
    assert!(matches!(too_big, Err(ServiceError::Invalid(_))), "{too_big:?}");
    let tmp = fixture.config.storage.attachments_dir().join("tmp");
    assert_eq!(std::fs::read_dir(&tmp).unwrap().count(), 0, "the partial upload is deleted");

    for bad in [".env", "a/b", "../x", ""] {
        assert!(
            matches!(releases.upload_asset(&repo, &alice, release.id, bad, "", &b"x"[..]).await, Err(ServiceError::Invalid(_))),
            "{bad}"
        );
    }
    assert!(matches!(releases.upload_asset(&repo, &alice, release.id, "app.zip", "", &b"x"[..]).await, Err(ServiceError::Conflict(_))));

    let anonymous = fixture.open(None).await.unwrap();
    let mut download = releases.open_asset(&anonymous, "v1", "app.zip").await.unwrap();
    let mut bytes = Vec::new();
    download.file.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes, b"0123456789");
    assert_eq!(releases.by_tag(&anonymous, "v1").await.unwrap().assets[0].download_count, 1);
    assert!(matches!(releases.open_asset(&anonymous, "v1", "nope.zip").await, Err(ServiceError::NotFound)));

    releases.delete_asset(&repo, release.id, asset.id).await.unwrap();
    assert!(!on_disk.exists(), "deleting a file removes its bytes");
}

#[tokio::test]
async fn the_number_of_files_per_release_is_limited() {
    let fixture = Fixture::new(|config| config.releases.max_assets_per_release = 1).await;
    let alice = fixture.user("alice").await;
    let repo = fixture.repo(&alice).await;
    let releases = &fixture.services.releases;
    let release = releases.create(&repo, &alice, input("v1", false)).await.unwrap();
    releases.upload_asset(&repo, &alice, release.id, "one", "", &b"1"[..]).await.unwrap();
    assert!(matches!(releases.upload_asset(&repo, &alice, release.id, "two", "", &b"2"[..]).await, Err(ServiceError::Invalid(_))));
}

#[tokio::test]
async fn deleting_releases_and_repositories_removes_their_files() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let repo = fixture.repo(&alice).await;
    let releases = &fixture.services.releases;
    releases.spawn_background();

    let first = releases.create(&repo, &alice, input("v1", false)).await.unwrap();
    let asset = releases.upload_asset(&repo, &alice, first.id, "a.txt", "text/plain", &b"a"[..]).await.unwrap();
    let repo_dir = fixture.config.storage.attachments_dir().join(repo.repo.repo.id.0.to_string());
    releases.delete(&repo, first.id).await.unwrap();
    assert!(!repo_dir.join(asset.id.0.to_string()).exists(), "deleting a release removes its files");

    let second = releases.create(&repo, &alice, input("v2", false)).await.unwrap();
    releases.upload_asset(&repo, &alice, second.id, "b.txt", "text/plain", &b"b"[..]).await.unwrap();
    assert!(repo_dir.exists());
    fixture.services.repo_settings.delete(&alice, &repo, "alice/demo").await.unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    while repo_dir.exists() {
        assert!(Instant::now() < deadline, "the attachments of a deleted repository are removed");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::timeout(Duration::from_secs(5), releases.shutdown()).await.unwrap();
}
