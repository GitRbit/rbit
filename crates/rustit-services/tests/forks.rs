//! Forks against real stores and bare repositories created with the `git` CLI.

use std::{path::Path, process::Command, sync::Arc};

use rustit_core::{Config, User, Visibility};
use rustit_services::{CreateRepository, DomainEvent, RepoContext, ServiceError, Services, forks::ForkInput};
use rustit_storage::{Database, KvStore, NewUser};

struct Fixture {
    dir: tempfile::TempDir,
    database: Database,
    services: Services,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rustit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rustit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        Self { dir, database, services }
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

    /// `alice/demo` with a description, a pushed `main`, a `feature` branch and tag `v1`.
    async fn parent(&self, alice: &User, visibility: Visibility) -> RepoContext {
        let input = CreateRepository { name: "demo".to_owned(), description: "The demo".to_owned(), visibility };
        self.services.repositories.create(alice, input).await.unwrap();
        let repo = self.services.repositories.open(Some(alice), "alice", "demo").await.unwrap();
        let work = self.dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("README.md"), "hello\n").unwrap();
        git(&work, &["init", "--quiet", "--initial-branch=main"]);
        git(&work, &["add", "README.md"]);
        git(&work, &["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--quiet", "-m", "initial"]);
        git(&work, &["tag", "v1"]);
        git(&work, &["branch", "feature"]);
        git(&work, &["push", "--quiet", repo.disk_path.to_str().unwrap(), "main", "feature", "v1"]);
        repo
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

#[tokio::test]
async fn a_fork_copies_refs_and_settings_and_remembers_its_parent() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let parent = fixture.parent(&alice, Visibility::Public).await;
    let as_bob = fixture.services.repositories.open(Some(&bob), "alice", "demo").await.unwrap();
    let mut events = fixture.services.events.subscribe();

    let fork = fixture.services.forks.fork(&bob, &as_bob, ForkInput::default()).await.unwrap();
    assert_eq!((fork.owner.name.as_str(), fork.repo.name.as_str()), ("bob", "demo"));
    assert_eq!((fork.repo.description.as_deref(), fork.repo.visibility), (Some("The demo"), Visibility::Public));
    assert_eq!(
        events.recv().await.unwrap(),
        DomainEvent::RepositoryForked { repo: fork.repo.id, parent: parent.repo.repo.id, actor: bob.id() }
    );

    let opened = fixture.services.repositories.open(None, "bob", "demo").await.unwrap();
    let parent_refs = parent.reader.ref_snapshot().await.unwrap();
    let fork_refs = opened.reader.ref_snapshot().await.unwrap();
    assert_eq!(fork_refs, parent_refs, "branches and tags are copied");
    assert_eq!(fork_refs.len(), 3);
    let config = std::fs::read_to_string(opened.disk_path.join("config")).unwrap();
    assert!(!config.contains("remote \"origin\""), "the fork does not point at its parent: {config}");

    let forks = &fixture.services.forks;
    assert_eq!(forks.parent(None, &opened).await.unwrap().map(|p| p.full_name()), Some("alice/demo".to_owned()));
    assert_eq!(forks.count(None, &parent).await.unwrap(), 1);
    assert_eq!(forks.list(None, &parent).await.unwrap()[0].full_name(), "bob/demo");

    // A second fork under the same name clashes; another name works.
    assert!(matches!(forks.fork(&bob, &as_bob, ForkInput::default()).await, Err(ServiceError::Conflict(_))));
    let renamed = forks.fork(&bob, &as_bob, ForkInput { owner: String::new(), name: "demo-2".into() }).await.unwrap();
    assert_eq!(renamed.repo.name.as_str(), "demo-2");
    // Organisations bob does not belong to are refused.
    let other = ForkInput { owner: "alice".into(), name: "copy".into() };
    assert!(matches!(forks.fork(&bob, &as_bob, other).await, Err(ServiceError::Invalid(_))));
}

#[tokio::test]
async fn forks_of_private_repositories_stay_private() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let parent = fixture.parent(&alice, Visibility::Private).await;

    let fork = fixture.services.forks.fork(&alice, &parent, ForkInput { owner: String::new(), name: "mine".into() }).await.unwrap();
    assert_eq!(fork.repo.visibility, Visibility::Private);
    assert!(matches!(fixture.services.repositories.open(Some(&bob), "alice", "mine").await, Err(ServiceError::NotFound)));
    assert_eq!(fixture.services.forks.count(Some(&bob), &parent).await.unwrap(), 0, "hidden forks are not counted");
    assert_eq!(fixture.services.forks.count(Some(&alice), &parent).await.unwrap(), 1);
}

#[tokio::test]
async fn deleting_the_parent_keeps_the_fork() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let parent = fixture.parent(&alice, Visibility::Public).await;
    let as_bob = fixture.services.repositories.open(Some(&bob), "alice", "demo").await.unwrap();
    fixture.services.forks.fork(&bob, &as_bob, ForkInput::default()).await.unwrap();

    fixture.services.repo_settings.delete(&parent, "alice/demo").await.unwrap();
    let fork = fixture.services.repositories.open(Some(&bob), "bob", "demo").await.unwrap();
    assert!(fixture.services.forks.parent(Some(&bob), &fork).await.unwrap().is_none());
    assert!(!fork.reader.is_empty().await.unwrap(), "the fork's history survives");
}
