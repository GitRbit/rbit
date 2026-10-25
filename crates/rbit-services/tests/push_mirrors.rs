//! Push mirrors against real stores, a temporary data directory and local bare
//! repositories as remotes (`file://`, admitted by `[push_mirrors] allow_local_urls`).

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::Duration,
};

use rbit_core::{Config, Permission, User, Visibility};
use rbit_services::{CreateRepository, RepoContext, ServiceError, Services, push_mirrors::PushMirrorInput};
use rbit_storage::{Database, NewUser};

struct Fixture {
    dir: tempfile::TempDir,
    database: Database,
    services: Services,
}

impl Fixture {
    async fn new(configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.updates.check_for_updates = false;
        config.storage.data_dir = dir.path().join("data");
        config.push_mirrors.allow_local_urls = true;
        config.push_mirrors.debounce_seconds = 0;
        configure(&mut config);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = rbit_storage::KvStore::open(&config.storage.kv_path()).unwrap();
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

    async fn repo(&self, owner: &User, name: &str) -> RepoContext {
        let input = CreateRepository { name: name.to_owned(), description: String::new(), visibility: Visibility::Public };
        self.services.repositories.create(owner, input).await.unwrap();
        self.services.repositories.open(Some(owner), owner.name().as_str(), name).await.unwrap()
    }

    /// An empty bare repository to push to, and its `file://` URL.
    fn remote(&self, name: &str) -> (PathBuf, String) {
        let path = self.dir.path().join(name);
        git(self.dir.path(), &["init", "-q", "--bare", name]);
        let url = format!("file:///{}", path.to_string_lossy().replace('\\', "/").trim_start_matches('/'));
        (path, url)
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .stderr(Stdio::inherit())
        .output()
        .expect("git is installed");
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn refs(repo: &Path) -> Vec<String> {
    git(repo, &["for-each-ref", "--format=%(refname)"]).lines().map(str::to_owned).collect()
}

/// Commits `file` on `branch` in a scratch clone and pushes it into `repo`'s directory.
fn push_commit(repo: &RepoContext, scratch: &Path, branch: &str, file: &str) {
    if !scratch.exists() {
        std::fs::create_dir_all(scratch).unwrap();
        git(scratch, &["init", "-q"]);
    }
    std::fs::write(scratch.join(file), file).unwrap();
    git(scratch, &["add", "."]);
    git(scratch, &["commit", "-q", "-m", file]);
    git(scratch, &["push", "-q", "--force", repo.disk_path.to_str().unwrap(), &format!("HEAD:refs/heads/{branch}")]);
}

fn input(url: &str) -> PushMirrorInput {
    PushMirrorInput { url: url.to_owned(), username: "bot".to_owned(), secret: "s3cret".to_owned(), enabled: true, sync_on_push: true }
}

#[tokio::test]
async fn a_push_mirror_receives_branches_and_tags_and_prunes_the_rest() {
    let fx = Fixture::new(|_| {}).await;
    let alice = fx.user("alice").await;
    let repo = fx.repo(&alice, "rocket").await;
    let scratch = fx.dir.path().join("scratch");
    push_commit(&repo, &scratch, "main", "a.txt");
    push_commit(&repo, &scratch, "feature", "b.txt");
    git(&repo.disk_path, &["tag", "v1", "main"]);
    git(&repo.disk_path, &["update-ref", "refs/pull/1/head", "feature"]);

    let (remote, url) = fx.remote("remote.git");
    let mirror = fx.services.push_mirrors.create(&repo, &alice, &input(&url)).await.unwrap();
    assert!(mirror.has_secret && mirror.last_attempt_at.is_none());
    fx.services.push_mirrors.push(mirror.id).await.unwrap();
    assert_eq!(refs(&remote), ["refs/heads/feature", "refs/heads/main", "refs/tags/v1"]);
    let pushed = &fx.services.push_mirrors.list(&repo).await.unwrap()[0];
    assert!(pushed.last_attempt_at.is_some() && pushed.last_success_at.is_some() && pushed.last_error.is_none());
    assert!(pushed.next_sync_at.seconds() > rbit_core::Timestamp::now().seconds() + 3_000, "next push after the interval");

    git(&repo.disk_path, &["branch", "-D", "feature"]);
    fx.services.push_mirrors.push(mirror.id).await.unwrap();
    assert_eq!(refs(&remote), ["refs/heads/main", "refs/tags/v1"], "branches deleted here are deleted there");
}

#[tokio::test]
async fn failures_are_recorded_for_the_settings_page() {
    let fx = Fixture::new(|_| {}).await;
    let alice = fx.user("alice").await;
    let repo = fx.repo(&alice, "rocket").await;
    push_commit(&repo, &fx.dir.path().join("scratch"), "main", "a.txt");
    let (remote, url) = fx.remote("remote.git");
    let mirror = fx.services.push_mirrors.create(&repo, &alice, &input(&url)).await.unwrap();
    std::fs::remove_dir_all(&remote).unwrap();

    fx.services.push_mirrors.push(mirror.id).await.unwrap();
    let failed = &fx.services.push_mirrors.list(&repo).await.unwrap()[0];
    assert!(failed.last_attempt_at.is_some() && failed.last_success_at.is_none());
    let error = failed.last_error.as_deref().unwrap();
    assert!(!error.contains("s3cret"), "the secret never appears in errors: {error}");
}

#[tokio::test]
async fn pushes_to_the_repository_are_mirrored_in_the_background() {
    let fx = Fixture::new(|_| {}).await;
    fx.services.spawn_background();
    let alice = fx.user("alice").await;
    let repo = fx.repo(&alice, "rocket").await;
    let (remote, url) = fx.remote("remote.git");
    let on_push = fx.services.push_mirrors.create(&repo, &alice, &input(&url)).await.unwrap();
    let (quiet_remote, quiet_url) = fx.remote("quiet.git");
    let quiet = PushMirrorInput { sync_on_push: false, ..input(&quiet_url) };
    fx.services.push_mirrors.create(&repo, &alice, &quiet).await.unwrap();

    // Both are pushed once right away (empty), then only the one that syncs on push follows.
    wait_until(|| async { fx.services.push_mirrors.list(&repo).await.unwrap().iter().all(|m| m.last_attempt_at.is_some()) }).await;
    let pending = fx.services.pushes.begin(&repo, Some(alice.id())).await;
    push_commit(&repo, &fx.dir.path().join("scratch"), "main", "a.txt");
    fx.services.pushes.complete(pending).await;
    wait_until(|| async { refs(&remote) == ["refs/heads/main"] }).await;
    assert_eq!(refs(&quiet_remote).len(), 0, "mirrors that do not sync on push wait for the interval");
    // The outcome is recorded right after git finishes.
    let pushed_at = rbit_core::Timestamp::now().seconds();
    wait_until(|| async {
        let mirrors = fx.services.push_mirrors.list(&repo).await.unwrap();
        let mirror = mirrors.iter().find(|m| m.id == on_push.id).unwrap();
        assert_eq!(mirror.last_error, None);
        mirror.last_success_at.is_some_and(|at| at.seconds() >= pushed_at - 1)
    })
    .await;
    fx.services.shutdown().await;
}

#[tokio::test]
async fn only_administrators_manage_push_mirrors_and_remotes_are_checked() {
    let fx = Fixture::new(|_| {}).await;
    let alice = fx.user("alice").await;
    let bob = fx.user("bob").await;
    let repo = fx.repo(&alice, "rocket").await;
    fx.services.repositories.create(&alice, CreateRepository { name: "x".into(), ..CreateRepository::default() }).await.unwrap();
    fx.database.repositories().grant_collaborator(repo.repo.repo.id, bob.id(), Permission::Write).await.unwrap();
    let as_bob = fx.services.repositories.open(Some(&bob), "alice", "rocket").await.unwrap();
    let (_remote, url) = fx.remote("remote.git");
    assert!(matches!(fx.services.push_mirrors.create(&as_bob, &bob, &input(&url)).await, Err(ServiceError::NotFound)));
    assert!(matches!(fx.services.push_mirrors.list(&as_bob).await, Err(ServiceError::NotFound)));

    let mirror = fx.services.push_mirrors.create(&repo, &alice, &input(&url)).await.unwrap();
    assert!(matches!(fx.services.push_mirrors.create(&repo, &alice, &input(&url)).await, Err(ServiceError::Conflict(_))));
    for bad in ["https://user:pw@example.com/a.git", "ssh://git@example.com/a.git", "http://127.0.0.1/a.git", "http://[::1]/a.git"] {
        let result = fx.services.push_mirrors.create(&repo, &alice, &input(bad)).await;
        assert!(matches!(result, Err(ServiceError::Invalid(_))), "{bad}: {result:?}");
    }
    let bad_name = PushMirrorInput { username: "a:b".into(), ..input("https://example.com/b.git") };
    assert!(matches!(fx.services.push_mirrors.create(&repo, &alice, &bad_name).await, Err(ServiceError::Invalid(_))));

    // Editing with a blank secret keeps it; another repository cannot reach the mirror.
    let edited = PushMirrorInput { secret: String::new(), enabled: false, ..input(&url) };
    let updated = fx.services.push_mirrors.update(&repo, mirror.id, &edited).await.unwrap();
    assert!(updated.has_secret && !updated.enabled);
    let other = fx.services.repositories.open(Some(&alice), "alice", "x").await.unwrap();
    assert!(matches!(fx.services.push_mirrors.update(&other, mirror.id, &edited).await, Err(ServiceError::NotFound)));
    assert!(matches!(fx.services.push_mirrors.sync_now(&other, mirror.id).await, Err(ServiceError::NotFound)));
    assert!(matches!(fx.services.push_mirrors.delete(&other, mirror.id).await, Err(ServiceError::NotFound)));
    fx.services.push_mirrors.delete(&repo, mirror.id).await.unwrap();

    let strict = Fixture::new(|config| config.push_mirrors.allow_local_urls = false).await;
    let carol = strict.user("carol").await;
    let repo = strict.repo(&carol, "rocket").await;
    let result = strict.services.push_mirrors.create(&repo, &carol, &input(&url)).await;
    assert!(matches!(result, Err(ServiceError::Invalid(_))), "file:// needs the test switch");
    let disabled = Fixture::new(|config| config.push_mirrors.enabled = false).await;
    let dave = disabled.user("dave").await;
    let repo = disabled.repo(&dave, "rocket").await;
    assert!(matches!(disabled.services.push_mirrors.list(&repo).await, Err(ServiceError::NotFound)));
}

async fn wait_until<F, Fut>(mut condition: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..200 {
        if condition().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the condition never held");
}
