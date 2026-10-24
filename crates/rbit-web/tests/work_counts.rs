//! How many pooled connections and statements each page takes, through the real router.
//!
//! The 16-connection pool is the limit under concurrent load (docs/performance-followups.md),
//! so every statement a page adds is a cost every visitor pays. These counts are exact:
//! a change that adds a query to one of these pages must update the expectation here.
//! They also check that the folded repository lookup still hides what the viewer may not
//! read, recomputing access on every request.

use std::{fmt::Write as _, path::Path, process::Command, sync::Arc};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use rbit_core::{Config, User, Visibility, membership::Permission};
use rbit_services::{CreateRepository, DomainEvent, Services, SignUp, forks::ForkInput, issues::NewIssueInput, pulls::NewPullInput};
use rbit_storage::{Database, KvStore, Work, count_work};
use tokio::sync::broadcast::{Receiver, error::TryRecvError};
use tower::ServiceExt;

fn git(directory: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(directory)
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
}

struct Fixture {
    directory: tempfile::TempDir,
    config: Arc<Config>,
    services: Services,
    router: Router,
    owner: User,
    reader: User,
    events: Receiver<DomainEvent>,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let events = services.events.subscribe();
        let user =
            |name: &str| SignUp { username: name.into(), email: format!("{name}@example.com"), password: "correct horse battery".into() };
        let owner = services.accounts.create_user(user("count-owner"), Some(false)).await.unwrap();
        let reader = services.accounts.create_user(user("count-reader"), Some(false)).await.unwrap();
        let router = rbit_web::router(rbit_web::AppState::new(Arc::clone(&config), services.clone()));
        Self { directory, config, services, router, owner, reader, events }
    }

    /// `owner/name` with a `main` branch and a `feature` branch one commit ahead.
    async fn repository(&self, name: &str, visibility: Visibility) {
        let created = self
            .services
            .repositories
            .create(&self.owner, CreateRepository { name: name.into(), description: String::new(), visibility })
            .await
            .unwrap();
        let source = self.directory.path().join(format!("source-{name}"));
        std::fs::create_dir(&source).unwrap();
        git(&source, &["init", "-q"]);
        std::fs::write(source.join("README.md"), "# Counted\n").unwrap();
        git(&source, &["add", "."]);
        git(&source, &["commit", "-qm", "initial"]);
        git(&source, &["checkout", "-qb", "feature"]);
        std::fs::write(source.join("lib.rs"), "fn main() {}\n").unwrap();
        git(&source, &["add", "."]);
        git(&source, &["commit", "-qm", "feature"]);
        git(&source, &["checkout", "-qb", "second", "main"]);
        std::fs::write(source.join("second.rs"), "fn second() {}\n").unwrap();
        git(&source, &["add", "."]);
        git(&source, &["commit", "-qm", "second"]);
        let bare = rbit_git::RepositoryStorage::new(self.config.storage.repositories_dir()).path_for(created.owner.id, created.repo.id);
        git(&bare, &["fetch", "-q", source.to_str().unwrap(), "main:main", "feature:feature", "second:second"]);
    }

    /// Delivers the events published so far to the notification service, as its
    /// background subscriber would.
    async fn deliver(&mut self) {
        loop {
            match self.events.try_recv() {
                Ok(event) => self.services.notifications.handle_event(&event).await.unwrap(),
                Err(TryRecvError::Empty | TryRecvError::Closed) => return,
                Err(TryRecvError::Lagged(_)) => panic!("lagged"),
            }
        }
    }

    async fn cookie(&self, user: &User) -> String {
        format!("rbit_session={}", self.services.accounts.start_session(user).await.unwrap().encode())
    }

    /// The status of `GET path` and the database work the request took.
    async fn get(&self, path: &str, cookie: Option<&str>) -> (StatusCode, Work) {
        let mut request = Request::builder().uri(path);
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        let request = request.body(Body::empty()).unwrap();
        let (response, work) = count_work(self.router.clone().oneshot(request)).await;
        let response = response.unwrap();
        let status = response.status();
        // The body streams nothing from the database, but read it to be sure.
        let (_, body_work) = count_work(axum::body::to_bytes(response.into_body(), usize::MAX)).await;
        assert_eq!(body_work, Work::default(), "{path}");
        (status, work)
    }

    /// The status and body of `GET path`.
    async fn page(&self, path: &str, cookie: Option<&str>) -> (StatusCode, String) {
        let mut request = Request::builder().uri(path);
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        let response = self.router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }
}

/// One line per page: `page viewer acquisitions statements cache-writes`.
fn report(rows: &[(&str, &str, Work)]) -> String {
    let mut report = String::new();
    for (page, viewer, work) in rows {
        let Work { acquisitions, statements, cache_writes } = work;
        writeln!(report, "{page:<26} {viewer:<9} {acquisitions:>3} {statements:>3} {cache_writes:>2}").unwrap();
    }
    report
}

/// `page viewer acquisitions statements cache-writes`. The signed-in viewer is neither
/// the owner nor a collaborator, so every access check runs in full.
const EXPECTED: &str = "
home                       anonymous   3   3  0
repository home            anonymous   2   2  1
issue view                 anonymous   4   4  0
pull conversation          anonymous   9   9  0
pull list                  anonymous  10  10  0
pull files                 anonymous   6   6  1
home                       signed-in   5   5  0
repository home            signed-in   3   3  0
issue view                 signed-in   8   8  0
pull conversation          signed-in  11  11  0
pull list                  signed-in  11  11  0
pull files                 signed-in   7   7  0
issue view, already read   signed-in   6   6  0
issue dashboard            signed-in   9   9  0
pull dashboard             signed-in   4   4  0
notifications              signed-in   4   4  0
";

/// `feature` into `main`.
fn pull_input() -> NewPullInput {
    NewPullInput {
        base_branch: "main".into(),
        head_branch: "feature".into(),
        title: "Counted pull".into(),
        body: String::new(),
        draft: false,
    }
}

#[tokio::test]
async fn pages_take_a_fixed_number_of_statements() {
    let mut fixture = Fixture::new().await;
    fixture.repository("public", Visibility::Public).await;
    // The reader's own repository, so their home lists repositories of their own too.
    let own = CreateRepository { name: "own".into(), description: String::new(), visibility: Visibility::Public };
    fixture.services.repositories.create(&fixture.reader, own).await.unwrap();
    let base = fixture.services.repositories.open(Some(&fixture.owner), "count-owner", "public").await.unwrap();
    let input = NewIssueInput { title: "Counted issue".into(), body: "Body".into(), ..NewIssueInput::default() };
    let issue = fixture.services.issues.create(&fixture.owner, &base, input).await.unwrap();
    let pull = pull_input();
    let (pull, _) = fixture.services.pulls.create(&fixture.owner, &base, &base, pull).await.unwrap();
    // A second pull request, so the list has rows of two.
    let second = NewPullInput { head_branch: "second".into(), title: "Second pull".into(), ..pull_input() };
    fixture.services.pulls.create(&fixture.owner, &base, &base, second).await.unwrap();
    // The reader takes part in the issue, then hears about the owner's reply.
    let as_reader = fixture.services.repositories.open(Some(&fixture.reader), "count-owner", "public").await.unwrap();
    let thread = fixture.services.timeline.open(Some(&fixture.reader), as_reader, issue.number).await.unwrap();
    fixture.services.timeline.comment(&fixture.reader, &thread, "Following").await.unwrap();
    let thread = fixture.services.timeline.open(Some(&fixture.owner), base.clone(), issue.number).await.unwrap();
    fixture.services.timeline.comment(&fixture.owner, &thread, "Reply").await.unwrap();
    // A second repository, so the dashboard and the inbox list rows of two.
    fixture.repository("other", Visibility::Public).await;
    for name in ["public", "other"] {
        let repo = fixture.services.repositories.open(Some(&fixture.reader), "count-owner", name).await.unwrap();
        let input = NewIssueInput { title: format!("Reader report in {name}"), ..NewIssueInput::default() };
        let issue = fixture.services.issues.create(&fixture.reader, &repo, input).await.unwrap();
        let repo = fixture.services.repositories.open(Some(&fixture.owner), "count-owner", name).await.unwrap();
        let thread = fixture.services.timeline.open(Some(&fixture.owner), repo, issue.number).await.unwrap();
        fixture.services.timeline.comment(&fixture.owner, &thread, "Thanks").await.unwrap();
    }
    fixture.deliver().await;
    let unread = fixture.services.notifications.inbox(&fixture.reader, true, 1).await.unwrap();
    assert!(!unread.notifications.is_empty(), "the reader has an unread notification");
    let cookie = fixture.cookie(&fixture.reader).await;

    let issue_path = format!("/count-owner/public/issues/{}", issue.number.get());
    let pull_path = format!("/count-owner/public/pull/{}", pull.number.get());
    let files_path = format!("{pull_path}/files");
    let mut rows = Vec::new();
    for (viewer, cookie) in [("anonymous", None), ("signed-in", Some(cookie.as_str()))] {
        for (page, path) in [
            ("home", "/"),
            ("repository home", "/count-owner/public"),
            ("issue view", issue_path.as_str()),
            ("pull conversation", pull_path.as_str()),
            ("pull list", "/count-owner/public/pulls"),
            ("pull files", files_path.as_str()),
        ] {
            let (status, work) = fixture.get(path, cookie).await;
            assert_eq!(status, StatusCode::OK, "{page} ({viewer})");
            rows.push((page, viewer, work));
        }
    }
    // The first view above marked the notification read; this one has nothing unread
    // about this thread (the reader still has unread notifications elsewhere).
    let (_, unread) = fixture.services.notifications.thread_state(fixture.reader.id(), issue.id).await.unwrap();
    assert!(!unread, "viewing the thread marked it read");
    assert!(fixture.services.notifications.has_unread(fixture.reader.id()).await.unwrap());
    let (status, work) = fixture.get(&issue_path, Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    rows.push(("issue view, already read", "signed-in", work));
    for (page, path) in [("issue dashboard", "/issues"), ("pull dashboard", "/pulls"), ("notifications", "/notifications?all=1")] {
        let (status, work) = fixture.get(path, Some(&cookie)).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        rows.push((page, "signed-in", work));
    }
    let report = report(&rows);
    eprintln!("{report}");
    assert_eq!(
        report,
        EXPECTED.trim_start_matches('\n'),
        "page work changed: update EXPECTED (and docs/ARCHITECTURE.md §7) if this is intended"
    );
}

/// Every repository page decides access from the row it loaded, on every request: a
/// private repository answers 404 to anyone who may not read it, and losing access takes
/// effect on the next request of the same session.
#[tokio::test]
async fn repository_pages_recheck_access_on_every_request() {
    let fixture = Fixture::new().await;
    fixture.repository("secret", Visibility::Private).await;
    let as_owner = fixture.services.repositories.open(Some(&fixture.owner), "count-owner", "secret").await.unwrap();
    let input = NewIssueInput { title: "Hidden issue".into(), ..NewIssueInput::default() };
    fixture.services.issues.create(&fixture.owner, &as_owner, input).await.unwrap();
    let pull = NewPullInput {
        base_branch: "main".into(),
        head_branch: "feature".into(),
        title: "Hidden pull".into(),
        body: String::new(),
        draft: false,
    };
    fixture.services.pulls.create(&fixture.owner, &as_owner, &as_owner, pull).await.unwrap();
    let cookie = fixture.cookie(&fixture.reader).await;
    let pages = [
        "/count-owner/secret",
        "/count-owner/secret/tree/main",
        "/count-owner/secret/commits",
        "/count-owner/secret/issues",
        "/count-owner/secret/issues/1",
        "/count-owner/secret/pulls",
        "/count-owner/secret/pull/2",
        "/count-owner/secret/pull/2/files",
    ];
    let expect = async |signed_in: StatusCode, why: &str| {
        for viewer in [None, Some(cookie.as_str())] {
            for path in pages {
                let wanted = if viewer.is_none() { StatusCode::NOT_FOUND } else { signed_in };
                let (got, body) = fixture.page(path, viewer).await;
                assert_eq!(got, wanted, "{path} ({why}, signed in: {})", viewer.is_some());
                if got == StatusCode::NOT_FOUND {
                    assert!(!body.contains("Hidden issue") && !body.contains("id=\"repo-subnav\""), "{path} leaks the repository");
                }
            }
        }
    };
    expect(StatusCode::NOT_FOUND, "never granted").await;

    fixture.services.collaborators.add(&fixture.owner, &as_owner, "count-reader", Permission::Read).await.unwrap();
    expect(StatusCode::OK, "collaborator").await;
    let (_, home) = fixture.page("/count-owner/secret", Some(&cookie)).await;
    assert!(home.contains("access as a collaborator."), "the sidebar offers to leave");
    // While they can read it, the reader forks it; the fork names its private parent.
    let as_reader = fixture.services.repositories.open(Some(&fixture.reader), "count-owner", "secret").await.unwrap();
    fixture.services.forks.fork(&fixture.reader, &as_reader, ForkInput::default()).await.unwrap();
    let (status, fork) = fixture.page("/count-reader/secret", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(fork.contains("href=\"/count-owner/secret\""), "a readable parent is named");

    fixture.services.collaborators.remove(&fixture.owner, &as_owner, "count-reader").await.unwrap();
    expect(StatusCode::NOT_FOUND, "access removed").await;
    let (status, fork) = fixture.page("/count-reader/secret", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "the fork itself stays readable to its owner");
    assert!(!fork.contains("count-owner/secret"), "a parent the viewer lost access to is not named");
}
