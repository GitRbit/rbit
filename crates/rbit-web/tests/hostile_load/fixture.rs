//! A public repository seeded with hostile content, served by the real router, and a
//! harness that fires hostile requests together with one ordinary read.

use std::{
    num::NonZeroUsize,
    path::Path,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use rbit_core::{Config, User, Visibility};
use rbit_identity::PasswordWork;
use rbit_kernel::BlockingBudget;
use rbit_services::{CreateRepository, Services, SignUp};
use rbit_storage::{Database, KvStore};
use tower::ServiceExt;

pub(super) const OWNER: &str = "hostile";
pub(super) const REPO: &str = "load";
pub(super) const PASSWORD: &str = "hostile-load-password-123";
/// Password jobs admitted at once.
pub(super) const PASSWORD_JOBS: usize = 2;
/// How long an ordinary read may take while hostile work is in flight. Generous for
/// debug builds on shared CI machines; the decisive assertion is that the ordinary read
/// finishes while hostile work is still running.
pub(super) const ORDINARY_BOUND: Duration = Duration::from_secs(10);
/// An ordinary blob that every test reads during the hostile load.
pub(super) const ORDINARY_PATH: &str = "/hostile/load/blob/main/ordinary.txt";

pub(super) fn git(directory: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(directory)
        .args(["-c", "user.name=Hostile Test", "-c", "user.email=hostile@example.com", "-c", "init.defaultBranch=main"])
        .args(["-c", "core.autocrlf=false", "-c", "commit.gpgsign=false"])
        .args(arguments)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {arguments:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// Writes `files` (relative path, content) under `root` and commits them.
pub(super) fn commit(root: &Path, files: &[(String, Vec<u8>)], message: &str) {
    for (path, content) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", message]);
}

/// Appends `commits` commits to `main` of the work tree at `root` with `git fast-import`
/// (far faster than one `git commit` each); `files(n)` gives commit `n`'s changed files.
pub(super) fn fast_history(root: &Path, commits: usize, files: impl Fn(usize) -> Vec<(String, Vec<u8>)>) {
    use std::io::Write as _;
    let mut stream = Vec::new();
    for number in 0..commits {
        let message = format!("History {number}");
        write!(stream, "commit refs/heads/main\ncommitter Hostile Test <hostile@example.com> {} +0000\n", 1_700_000_000 + number).unwrap();
        write!(stream, "data {}\n{message}\n", message.len()).unwrap();
        if number == 0 {
            writeln!(stream, "from refs/heads/main^0").unwrap();
        }
        for (path, content) in files(number) {
            write!(stream, "M 100644 inline {path}\ndata {}\n", content.len()).unwrap();
            stream.extend_from_slice(&content);
            stream.push(b'\n');
        }
    }
    let mut child = Command::new("git")
        .current_dir(root)
        .args(["fast-import", "--quiet", "--force"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&stream).unwrap();
    assert!(child.wait().unwrap().success(), "git fast-import");
    git(root, &["reset", "-q", "--hard", "main"]);
}

pub(super) struct Fixture {
    _directory: tempfile::TempDir,
    _kv: KvStore,
    pub(super) database: Database,
    pub(super) services: Services,
    pub(super) passwords: PasswordWork,
    pub(super) user: User,
    router: Router,
}

/// One answer, and when it arrived.
#[derive(Debug)]
pub(super) struct Reply {
    pub(super) status: StatusCode,
    pub(super) headers: HeaderMap,
    pub(super) body: String,
    pub(super) elapsed: Duration,
    pub(super) finished: Instant,
}

impl Fixture {
    /// `configure` adjusts the configuration; `seed` commits content into a work tree
    /// whose `main` becomes the repository's `main`.
    pub(super) async fn new(configure: impl FnOnce(&mut Config), seed: impl FnOnce(&Path)) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        config.security.max_password_jobs = PASSWORD_JOBS;
        configure(&mut config);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let passwords = PasswordWork::new(BlockingBudget::new(NonZeroUsize::new(PASSWORD_JOBS).unwrap()));
        let services = Services::with_password_work(config.clone(), &database, &kv, passwords.clone()).await.unwrap();
        let signup = SignUp { username: OWNER.into(), email: "hostile@example.com".into(), password: PASSWORD.into() };
        let user = services.accounts.create_user(signup, Some(false)).await.unwrap();
        let repo = services
            .repositories
            .create(&user, CreateRepository { name: REPO.into(), description: String::new(), visibility: Visibility::Public })
            .await
            .unwrap();
        let source = directory.path().join("source");
        std::fs::create_dir(&source).unwrap();
        git(&source, &["init", "-q"]);
        commit(&source, &[("ordinary.txt".to_owned(), b"An ordinary file.\n".to_vec())], "Ordinary content");
        seed(&source);
        let bare = rbit_git::RepositoryStorage::new(config.storage.repositories_dir()).path_for(repo.owner.id, repo.repo.id);
        git(&bare, &["fetch", "-q", source.to_str().unwrap(), "main:main"]);
        let router = rbit_web::router(rbit_web::AppState::new(config, services.clone()));
        Self { _directory: directory, _kv: kv, database, services, passwords, user, router }
    }

    pub(super) fn router(&self) -> Router {
        self.router.clone()
    }

    pub(super) async fn get(&self, path: &str) -> Reply {
        send(self.router(), get(path)).await
    }
}

pub(super) fn get(path: &str) -> Request<Body> {
    Request::builder().uri(path).body(Body::empty()).unwrap()
}

pub(super) fn form(path: &str, body: &str, cookie: Option<&str>) -> Request<Body> {
    let mut request = Request::builder().method(Method::POST).uri(path).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    request.body(Body::from(body.to_owned())).unwrap()
}

pub(super) async fn send(router: Router, request: Request<Body>) -> Reply {
    let started = Instant::now();
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let finished = Instant::now();
    Reply { status, headers, body: String::from_utf8_lossy(&bytes).into_owned(), elapsed: finished - started, finished }
}

/// Sends every hostile request at once and, once they are in flight, the ordinary read.
/// Returns the hostile replies in order and the ordinary one.
pub(super) async fn under_load(router: &Router, hostile: Vec<Request<Body>>, ordinary: Request<Body>) -> (Vec<Reply>, Reply) {
    let tasks: Vec<_> = hostile.into_iter().map(|request| tokio::spawn(send(router.clone(), request))).collect();
    tokio::time::sleep(Duration::from_millis(20)).await;
    let ordinary = send(router.clone(), ordinary).await;
    let mut replies = Vec::with_capacity(tasks.len());
    for task in tasks {
        replies.push(tokio::time::timeout(Duration::from_secs(120), task).await.expect("hostile request is bounded").unwrap());
    }
    (replies, ordinary)
}

/// The ordinary read succeeded within [`ORDINARY_BOUND`], before the slowest hostile
/// request finished: it was served alongside the hostile work, not after it.
pub(super) fn assert_served_alongside(hostile: &[Reply], ordinary: &Reply) {
    assert_eq!(ordinary.status, StatusCode::OK, "{}", ordinary.body);
    assert!(ordinary.elapsed < ORDINARY_BOUND, "the ordinary read took {:?}", ordinary.elapsed);
    let last = hostile.iter().map(|reply| reply.finished).max().expect("hostile requests");
    assert!(
        ordinary.finished < last,
        "the ordinary read ({:?}) waited for every hostile request (slowest {:?})",
        ordinary.elapsed,
        hostile.iter().map(|reply| reply.elapsed).max()
    );
}

/// A refusal is immediate and retryable.
pub(super) fn assert_refused(reply: &Reply, status: StatusCode) {
    assert_eq!(reply.status, status, "{}", reply.body);
    assert!(reply.headers.contains_key(header::RETRY_AFTER));
}
