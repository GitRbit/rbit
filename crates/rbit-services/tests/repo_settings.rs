//! Repository settings and stars against real, temporary `SQLite`/redb stores and bare repositories.

use std::{path::Path, process::Command, sync::Arc};

use rbit_core::{Config, User, Visibility, membership::Permission};
use rbit_services::{CreateRepository, DomainEvent, RepoContext, ServiceError, Services, repo_settings::UpdateRepository};
use rbit_storage::{Database, KvStore, NewUser};

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

    /// Pushes one commit to each of `branches` using the `git` CLI.
    fn push_branches(&self, repo: &RepoContext, branches: &[&str]) {
        let work = self.dir.path().join(format!("work-{}", repo.repo.repo.id.0));
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("README.md"), "hello\n").unwrap();
        git(&work, &["init", "--quiet", "--initial-branch=scratch"]);
        git(&work, &["add", "README.md"]);
        git(&work, &["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--quiet", "-m", "initial"]);
        for branch in branches {
            git(&work, &["push", "--quiet", repo.disk_path.to_str().unwrap(), &format!("HEAD:refs/heads/{branch}")]);
        }
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
    assert!(status.success(), "git {args:?} failed");
}

fn head_of(repo: &RepoContext) -> String {
    std::fs::read_to_string(repo.disk_path.join("HEAD")).unwrap()
}

fn general(description: &str, visibility: Visibility, default_branch: Option<&str>) -> UpdateRepository {
    UpdateRepository { description: description.to_owned(), visibility, default_branch: default_branch.map(str::to_owned) }
}

#[tokio::test]
async fn old_account_names_authorize_the_current_repository_identity() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let repo = fixture.repo(&alice, "secret", Visibility::Private).await;
    fixture.database.accounts().rename(alice.id(), &"renamed".parse().unwrap()).await.unwrap();
    let repositories = &fixture.services.repositories;
    assert!(repositories.moved_to(None, "alice", "secret").await.unwrap().is_none());
    assert!(repositories.moved_to(Some(&bob), "alice", "secret").await.unwrap().is_none());
    assert!(matches!(repositories.open_following_renames(Some(&bob), "alice", "secret").await, Err(ServiceError::NotFound)));
    let opened = repositories.open_following_renames(Some(&alice), "alice", "secret.git").await.unwrap();
    assert_eq!(opened.repo.repo.id, repo.repo.repo.id);
    assert_eq!(opened.repo.full_name(), "renamed/secret");
    assert_eq!(opened.disk_path, repo.disk_path);
    assert!(repositories.moved_to(Some(&alice), "renamed", "secret").await.unwrap().is_none());
}

#[tokio::test]
async fn update_description_visibility_and_default_branch() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let repo = fixture.repo(&alice, "project", Visibility::Public).await;
    fixture.push_branches(&repo, &["main", "develop"]);

    let settings = &fixture.services.repo_settings;
    settings.update(&repo, general("  A tidy project  ", Visibility::Private, Some("develop"))).await.unwrap();

    let reopened = fixture.open(Some(&alice), "alice", "project").await.unwrap();
    assert_eq!(reopened.repo.repo.description.as_deref(), Some("A tidy project"));
    assert_eq!(reopened.repo.repo.visibility, Visibility::Private);
    assert_eq!(reopened.repo.repo.default_branch, "develop");
    assert_eq!(head_of(&reopened), "ref: refs/heads/develop\n");
    assert!(matches!(fixture.open(None, "alice", "project").await, Err(ServiceError::NotFound)), "now private");

    // Unknown branches are rejected and change nothing.
    let missing = settings.update(&reopened, general("", Visibility::Public, Some("nope"))).await;
    assert!(matches!(missing, Err(ServiceError::Invalid(_))));
    assert_eq!(head_of(&reopened), "ref: refs/heads/develop\n");

    // `None` keeps the default branch; a blank description clears it.
    settings.update(&reopened, general("", Visibility::Public, None)).await.unwrap();
    let reopened = fixture.open(None, "alice", "project").await.unwrap();
    assert_eq!(reopened.repo.repo.description, None);
    assert_eq!(reopened.repo.repo.default_branch, "develop");

    let as_alice = fixture.open(Some(&alice), "alice", "project").await.unwrap();
    let long = settings.update(&as_alice, general(&"x".repeat(351), Visibility::Public, None)).await;
    assert!(matches!(long, Err(ServiceError::Invalid(_))));
}

#[tokio::test]
async fn empty_repository_keeps_its_default_branch() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let repo = fixture.repo(&alice, "empty", Visibility::Public).await;
    let initial = repo.repo.repo.default_branch.clone();

    let result = fixture.services.repo_settings.update(&repo, general("", Visibility::Public, Some("other"))).await;
    assert!(matches!(result, Err(ServiceError::Invalid(_))));
    fixture.services.repo_settings.update(&repo, general("", Visibility::Public, Some(&initial))).await.unwrap();
    assert_eq!(head_of(&repo), format!("ref: refs/heads/{initial}\n"));
}

#[tokio::test]
async fn rename_repository() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let repo = fixture.repo(&alice, "old-name", Visibility::Public).await;
    fixture.repo(&alice, "taken", Visibility::Public).await;
    let settings = &fixture.services.repo_settings;

    assert!(matches!(settings.rename(&repo, "TAKEN").await, Err(ServiceError::Conflict(_))));
    assert!(matches!(settings.rename(&repo, "bad name").await, Err(ServiceError::Invalid(_))));
    assert!(matches!(settings.rename(&repo, "settings").await, Err(ServiceError::Invalid(_))));

    let renamed = settings.rename(&repo, " new-name ").await.unwrap();
    assert_eq!(renamed.as_str(), "new-name");
    assert!(matches!(fixture.open(None, "alice", "old-name").await, Err(ServiceError::NotFound)));
    let reopened = fixture.open(Some(&alice), "alice", "new-name").await.unwrap();
    assert_eq!(reopened.repo.repo.id, repo.repo.repo.id);
    assert_eq!(reopened.disk_path, repo.disk_path, "on-disk paths use ids");

    // Changing only the letter case is allowed.
    assert_eq!(settings.rename(&reopened, "New-Name").await.unwrap().as_str(), "New-Name");
    assert_eq!(settings.rename(&reopened, "new-name").await.unwrap().as_str(), "new-name");
}

#[tokio::test]
async fn delete_repository() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let repo = fixture.repo(&alice, "doomed", Visibility::Public).await;
    fixture.services.stars.star(&bob, &fixture.open(Some(&bob), "alice", "doomed").await.unwrap()).await.unwrap();
    assert!(repo.disk_path.exists());

    let wrong = fixture.services.repo_settings.delete(&alice, &repo, "doomed").await;
    assert!(matches!(wrong, Err(ServiceError::Invalid(_))));
    assert!(repo.disk_path.exists());

    fixture.services.repo_settings.delete(&alice, &repo, "alice/doomed").await.unwrap();
    assert!(!repo.disk_path.exists());
    assert!(matches!(fixture.open(Some(&alice), "alice", "doomed").await, Err(ServiceError::NotFound)));
    assert!(fixture.services.stars.starred_by(Some(&bob), &bob.account).await.unwrap().is_empty(), "stars cascade");
}

#[tokio::test]
async fn creating_and_deleting_repositories_publish_events() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let mut events = fixture.services.events.subscribe();

    let repo = fixture.repo(&alice, "short-lived", Visibility::Private).await;
    let id = repo.repo.repo.id;
    assert_eq!(events.recv().await.unwrap(), DomainEvent::RepositoryCreated { repo: id, actor: alice.id() });

    fixture.services.repo_settings.delete(&alice, &repo, "alice/short-lived").await.unwrap();
    assert_eq!(
        events.recv().await.unwrap(),
        DomainEvent::RepositoryDeleted { repo: id, owner: alice.id(), full_name: "alice/short-lived".to_owned(), actor: alice.id() }
    );
}

#[tokio::test]
async fn only_admins_change_settings() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let root = fixture.user("root", true).await;
    fixture.repo(&alice, "project", Visibility::Public).await;
    let settings = &fixture.services.repo_settings;

    let as_bob = fixture.open(Some(&bob), "alice", "project").await.unwrap();
    assert!(!as_bob.access.can_admin());
    assert!(matches!(settings.update(&as_bob, general("", Visibility::Private, None)).await, Err(ServiceError::Forbidden)));
    assert!(matches!(settings.rename(&as_bob, "mine").await, Err(ServiceError::Forbidden)));
    assert!(matches!(settings.delete(&bob, &as_bob, "alice/project").await, Err(ServiceError::Forbidden)));

    let as_root = fixture.open(Some(&root), "alice", "project").await.unwrap();
    settings.update(&as_root, general("Moderated", Visibility::Public, None)).await.unwrap();
}

#[tokio::test]
async fn old_names_resolve_only_for_readers() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let carol = fixture.user("carol", false).await;
    let root = fixture.user("root", true).await;
    let repo = fixture.repo(&alice, "secret", Visibility::Private).await;
    fixture.database.repositories().grant_collaborator(repo.repo.repo.id, carol.id(), Permission::Read).await.unwrap();
    fixture.services.repo_settings.rename(&repo, "hidden").await.unwrap();
    let repositories = &fixture.services.repositories;

    // `open` matches the current name only; the Git transports follow old ones.
    assert!(matches!(fixture.open(Some(&alice), "alice", "secret").await, Err(ServiceError::NotFound)));
    for reader in [&alice, &carol, &root] {
        let moved = repositories.moved_to(Some(reader), "alice", "secret").await.unwrap().unwrap();
        assert_eq!(moved.full_name(), "alice/hidden", "{} may read it", reader.name());
        let followed = repositories.open_following_renames(Some(reader), "alice", "secret.git").await.unwrap();
        assert_eq!(followed.repo.repo.id, repo.repo.repo.id);
    }
    for outsider in [Some(&bob), None] {
        assert!(repositories.moved_to(outsider, "alice", "secret").await.unwrap().is_none());
        assert!(matches!(repositories.open_following_renames(outsider, "alice", "secret").await, Err(ServiceError::NotFound)));
    }
    assert!(repositories.moved_to(Some(&alice), "alice", "hidden").await.unwrap().is_none(), "the current name has not moved");
    assert!(repositories.moved_to(Some(&alice), "alice", "unknown").await.unwrap().is_none());

    // A new repository at the old name wins over it.
    let newcomer = fixture.repo(&alice, "secret", Visibility::Private).await;
    assert!(repositories.moved_to(Some(&alice), "alice", "secret").await.unwrap().is_none());
    let followed = repositories.open_following_renames(Some(&alice), "alice", "secret").await.unwrap();
    assert_eq!(followed.repo.repo.id, newcomer.repo.repo.id);
}

// ------------------------------------------------------------------ stars

#[tokio::test]
async fn star_and_unstar() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let repo = fixture.repo(&alice, "project", Visibility::Public).await;
    let id = repo.repo.repo.id;
    let stars = &fixture.services.stars;

    let summary = stars.summary(id, None).await.unwrap();
    assert_eq!((summary.count, summary.starred_by_viewer), (0, false));

    let summary = stars.star(&alice, &repo).await.unwrap();
    assert_eq!((summary.count, summary.starred_by_viewer), (1, true));
    let summary = stars.star(&alice, &repo).await.unwrap();
    assert_eq!(summary.count, 1, "starring twice is idempotent");

    let as_bob = fixture.open(Some(&bob), "alice", "project").await.unwrap();
    stars.star(&bob, &as_bob).await.unwrap();
    let summary = stars.summary(id, Some(&bob)).await.unwrap();
    assert_eq!((summary.count, summary.starred_by_viewer), (2, true));
    let summary = stars.summary(id, None).await.unwrap();
    assert_eq!((summary.count, summary.starred_by_viewer), (2, false));

    let gazers: Vec<_> = stars.stargazers(&repo).await.unwrap().into_iter().map(|s| s.account.name.to_string()).collect();
    assert_eq!(gazers.len(), 2);
    assert!(gazers.contains(&"alice".to_owned()) && gazers.contains(&"bob".to_owned()));

    let summary = stars.unstar(&alice, &repo).await.unwrap();
    assert_eq!((summary.count, summary.starred_by_viewer), (1, false));
    let summary = stars.unstar(&alice, &repo).await.unwrap();
    assert_eq!(summary.count, 1, "unstarring twice is idempotent");

    let starred = stars.starred_by(None, &bob.account).await.unwrap();
    assert_eq!(starred.len(), 1);
    assert_eq!(starred[0].full_name(), "alice/project");
}

#[tokio::test]
async fn private_repositories_stay_hidden() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let root = fixture.user("root", true).await;
    let secret = fixture.repo(&alice, "secret", Visibility::Private).await;
    let stars = &fixture.services.stars;

    // Bob can't even obtain a context for the repository, so he can't star it.
    assert!(matches!(fixture.open(Some(&bob), "alice", "secret").await, Err(ServiceError::NotFound)));

    stars.star(&alice, &secret).await.unwrap();
    assert_eq!(stars.starred_by(Some(&alice), &alice.account).await.unwrap().len(), 1, "owners see their own");
    assert_eq!(stars.starred_by(Some(&root), &alice.account).await.unwrap().len(), 1, "admins see everything");
    assert!(stars.starred_by(Some(&bob), &alice.account).await.unwrap().is_empty());
    assert!(stars.starred_by(None, &alice.account).await.unwrap().is_empty());
}
