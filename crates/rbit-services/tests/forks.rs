//! Forks against real stores and bare repositories created with the `git` CLI.

use std::{path::Path, process::Command, sync::Arc};

use rbit_core::{Config, PushSource, RefUpdate, User, Visibility, protection::RuleSettings};
use rbit_services::protection::ProtectionInput;
use rbit_services::{CreateRepository, DomainEvent, RepoContext, ServiceError, Services, forks::ForkInput};
use rbit_storage::{Database, KvStore, NewUser};

#[path = "forks/templates.rs"]
mod templates;

struct Fixture {
    dir: tempfile::TempDir,
    database: Database,
    services: Services,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
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
    // Pages, newest first, with the total.
    let (page, total) = forks.page(None, &parent, 1, 0).await.unwrap();
    assert_eq!((page[0].full_name(), total), ("bob/demo-2".to_owned(), 2));
    let (page, _) = forks.page(None, &parent, 1, 1).await.unwrap();
    assert_eq!(page[0].full_name(), "bob/demo");
    assert!(forks.page(None, &parent, 1, 2).await.unwrap().0.is_empty());
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

    // Neither the fork nor a fork of the fork can be made public; a private fork of a
    // public repository can.
    let public = |description: &str| rbit_services::repo_settings::UpdateRepository {
        description: description.to_owned(),
        visibility: Visibility::Public,
        default_branch: None,
    };
    let mine = fixture.services.repositories.open(Some(&alice), "alice", "mine").await.unwrap();
    let result = fixture.services.repo_settings.update(&mine, public("")).await;
    assert!(matches!(result, Err(ServiceError::Invalid(_))), "{result:?}");
    let nested = fixture.services.forks.fork(&alice, &mine, ForkInput { owner: String::new(), name: "nested".into() }).await.unwrap();
    assert_eq!(nested.repo.visibility, Visibility::Private);
    let nested = fixture.services.repositories.open(Some(&alice), "alice", "nested").await.unwrap();
    let result = fixture.services.repo_settings.update(&nested, public("")).await;
    assert!(matches!(result, Err(ServiceError::Invalid(_))), "the whole network is checked: {result:?}");
    let private = rbit_services::repo_settings::UpdateRepository {
        description: "still private".into(),
        visibility: Visibility::Private,
        default_branch: None,
    };
    fixture.services.repo_settings.update(&nested, private).await.unwrap();

    fixture.services.repo_settings.update(&parent, public("now public")).await.unwrap();
    fixture.services.repo_settings.update(&mine, public("")).await.unwrap();
    let nested = fixture.services.repositories.open(Some(&alice), "alice", "nested").await.unwrap();
    fixture.services.repo_settings.update(&nested, public("")).await.unwrap();
}

#[tokio::test]
async fn deleting_the_parent_keeps_the_fork() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let parent = fixture.parent(&alice, Visibility::Public).await;
    let as_bob = fixture.services.repositories.open(Some(&bob), "alice", "demo").await.unwrap();
    fixture.services.forks.fork(&bob, &as_bob, ForkInput::default()).await.unwrap();

    fixture.services.repo_settings.delete(&alice, &parent, "alice/demo").await.unwrap();
    let fork = fixture.services.repositories.open(Some(&bob), "bob", "demo").await.unwrap();
    assert!(fixture.services.forks.parent(Some(&bob), &fork).await.unwrap().is_none());
    assert!(!fork.reader.is_empty().await.unwrap(), "the fork's history survives");
}

#[tokio::test]
async fn sync_fast_forwards_only_the_expected_default_branch_and_reports_a_push() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let parent = fixture.parent(&alice, Visibility::Public).await;
    let services = &fixture.services;
    let as_bob = services.repositories.open(Some(&bob), "alice", "demo").await.unwrap();
    services.forks.fork(&bob, &as_bob, ForkInput::default()).await.unwrap();
    let fork = services.repositories.open(Some(&bob), "bob", "demo").await.unwrap();
    let work = fixture.dir.path().join("work");
    let initial = services.forks.sync_status(&bob, &fork).await.unwrap().unwrap();
    assert_eq!((initial.ahead, initial.behind), (0, 0));

    let commit =
        |message| git(&work, &["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--allow-empty", "-qm", message]);
    commit("upstream one");
    git(&work, &["push", "--quiet", parent.disk_path.to_str().unwrap(), "main"]);
    let stale = services.forks.sync_status(&bob, &fork).await.unwrap().unwrap();
    assert_eq!((stale.ahead, stale.behind), (0, 1));
    commit("upstream two");
    git(&work, &["push", "--quiet", parent.disk_path.to_str().unwrap(), "main"]);
    assert!(matches!(services.forks.sync(&bob, &fork, &stale.expected).await, Err(ServiceError::Invalid(_))));

    let settings = RuleSettings { require_pull_request: true, ..RuleSettings::default() };
    let rule =
        services.protection.create(&bob, &fork, &ProtectionInput { pattern: "main".into(), settings, ..Default::default() }).await.unwrap();
    let blocked = services.forks.sync_status(&bob, &fork).await.unwrap().unwrap();
    assert_eq!((blocked.ahead, blocked.behind), (0, 2));
    assert!(blocked.refusal.is_some());
    assert!(matches!(services.forks.sync(&bob, &fork, &blocked.expected).await, Err(ServiceError::Invalid(_))));
    services.protection.delete(&bob, &fork, rule.id).await.unwrap();

    let mut events = services.events.subscribe();
    services.forks.sync(&bob, &fork, &blocked.expected).await.unwrap();
    assert_eq!(
        events.recv().await.unwrap(),
        DomainEvent::RepositoryPushed {
            repo: fork.repo.repo.id,
            source: PushSource::User(bob.id()),
            updates: vec![RefUpdate {
                name: "refs/heads/main".into(),
                before: Some(blocked.expected.fork_tip),
                after: Some(blocked.expected.upstream_tip)
            }],
        }
    );
    let current = services.forks.sync_status(&bob, &fork).await.unwrap().unwrap();
    assert_eq!((current.ahead, current.behind), (0, 0));
    assert_eq!(fork.reader.branch_commit_id("feature".into()).await.unwrap().unwrap(), initial.expected.fork_tip);

    git(&work, &["checkout", "--quiet", "feature"]);
    commit("fork only");
    git(&work, &["push", "--quiet", "--force", fork.disk_path.to_str().unwrap(), "feature:refs/heads/main"]);
    let divergent = services.forks.sync_status(&bob, &fork).await.unwrap().unwrap();
    assert_eq!((divergent.ahead, divergent.behind), (1, 2));
    assert!(matches!(services.forks.sync(&bob, &fork, &divergent.expected).await, Err(ServiceError::Invalid(_))));
    assert_eq!(fork.reader.branch_commit_id("main".into()).await.unwrap().unwrap(), divergent.expected.fork_tip);

    for repo in [&fork, &parent] {
        services
            .repo_settings
            .update(
                repo,
                rbit_services::repo_settings::UpdateRepository {
                    description: String::new(),
                    visibility: Visibility::Private,
                    default_branch: None,
                },
            )
            .await
            .unwrap();
    }
    let private_fork = services.repositories.open(Some(&bob), "bob", "demo").await.unwrap();
    let private_parent = services.repositories.open(Some(&alice), "alice", "demo").await.unwrap();
    let second_fork =
        services.forks.fork(&alice, &private_parent, ForkInput { owner: String::new(), name: "private-fork".into() }).await.unwrap();
    let second_fork = services.repositories.open(Some(&alice), "alice", second_fork.repo.name.as_str()).await.unwrap();
    assert!(
        services.forks.sync_status(&alice, &second_fork).await.unwrap().is_none(),
        "read access to both private repositories is insufficient"
    );
    assert!(services.forks.sync_status(&bob, &private_fork).await.unwrap().is_none());
    assert!(matches!(services.forks.sync(&bob, &private_fork, &divergent.expected).await, Err(ServiceError::NotFound)));
}

mod ownership;
