//! Actions workstream U: the Actions tab, the runs list with its filters and the run page,
//! and what writers and readers may do to a run.

use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use rbit_core::{Config, Timestamp, User, actions::RunEvent};
use rbit_services::{CreateRepository, Services, SignUp};
use rbit_storage::{
    Database, KvStore,
    sqlite::actions_runs::{NewJob, NewRun, RunStart},
};
use rbit_web::AppState;
use tower::ServiceExt;

struct Fixture {
    _directory: tempfile::TempDir,
    database: Database,
    services: Services,
    router: Router,
    alice: User,
    /// Alice's session cookie; she owns `alice/rocket` (public).
    owner: String,
    /// Bob's: he may read the repository, nothing more.
    reader: String,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        config.actions.enabled = true;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(config.clone(), &database, &kv).await.unwrap();
        let sign_up =
            |name: &str| SignUp { username: name.into(), email: format!("{name}@example.com"), password: "correct horse battery".into() };
        let alice = services.accounts.create_user(sign_up("alice"), Some(true)).await.unwrap();
        let bob = services.accounts.create_user(sign_up("bob"), Some(false)).await.unwrap();
        let input = CreateRepository { name: "rocket".into(), description: String::new(), visibility: rbit_core::Visibility::Public };
        services.repositories.create(&alice, input).await.unwrap();
        let owner = format!("rbit_session={}", services.accounts.start_session(&alice).await.unwrap().encode());
        let reader = format!("rbit_session={}", services.accounts.start_session(&bob).await.unwrap().encode());
        let router = rbit_web::router(AppState::new(config, services.clone()));
        Self { _directory: directory, database, services, router, alice, owner, reader }
    }

    async fn send(&self, method: Method, path: &str, cookie: &str, body: &str) -> (StatusCode, String, Option<String>) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::COOKIE, cookie)
            .header(header::HOST, "localhost:3000")
            .header(header::ORIGIN, "http://localhost:3000")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body.to_owned()))
            .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let location = response.headers().get(header::LOCATION).map(|value| value.to_str().unwrap().to_owned());
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8_lossy(&body).into_owned(), location)
    }

    async fn page(&self, path: &str, cookie: &str) -> String {
        let (status, body, _) = self.send(Method::GET, path, cookie, "").await;
        assert_eq!(status, StatusCode::OK, "{path}");
        body
    }

    async fn repo_id(&self) -> rbit_core::RepoId {
        self.database.repositories().find_by_path("alice", "rocket").await.unwrap().unwrap().repo.id
    }

    /// Run `#n` of `alice/rocket` with jobs `build` and `test` (needing `build`).
    async fn run(&self, workflow: &str, title: &str, event: RunEvent) -> rbit_core::actions::Run {
        let (needs, labels) = (vec!["build".to_owned()], vec!["ubuntu-latest".to_owned()]);
        let jobs = [
            NewJob { key: "build", name: "build", matrix: None, needs: &[], runs_on: &labels, permissions: None, payload: b"{}" },
            NewJob { key: "test", name: "test", matrix: None, needs: &needs, runs_on: &labels, permissions: None, payload: b"{}" },
        ];
        let new = NewRun {
            repo: self.repo_id().await,
            workflow_path: workflow,
            workflow_name: "CI",
            title,
            event,
            reference: "refs/heads/main",
            sha: "0123456789abcdef0123456789abcdef01234567",
            actor: Some(self.alice.id()),
            trigger_actor: Some(self.alice.id()),
            pull_number: None,
            is_fork: false,
            trusted: true,
            concurrency_group: None,
            cancel_in_progress: false,
            event_payload: "{}",
            dispatch_inputs: None,
            start: RunStart::Runnable,
            jobs: &jobs,
            error: None,
            now: Timestamp::now(),
        };
        self.database.actions_runs().create_run(&new).await.unwrap()
    }
}

#[tokio::test]
async fn the_tab_and_pages_exist_only_for_repositories_with_actions_on() {
    let fixture = Fixture::new().await;
    let home = fixture.page("/alice/rocket", &fixture.owner).await;
    assert!(!home.contains(r#"data-repo-tab="actions""#), "the repository has Actions off");
    let (status, ..) = fixture.send(Method::GET, "/alice/rocket/actions", &fixture.owner, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    fixture.database.actions_settings().set_enabled(fixture.repo_id().await, true, Timestamp::now()).await.unwrap();
    let home = fixture.page("/alice/rocket", &fixture.reader).await;
    assert!(home.contains(r#"data-repo-tab="actions""#));
    let runs = fixture.page("/alice/rocket/actions", &fixture.reader).await;
    assert!(runs.contains("No workflow runs yet"));
    assert!(runs.contains(r#"data-repo-tab="actions" data-preload="visible" href="/alice/rocket/actions" aria-current="page""#));
}

#[tokio::test]
async fn runs_are_listed_newest_first_and_filtered() {
    let fixture = Fixture::new().await;
    fixture.database.actions_settings().set_enabled(fixture.repo_id().await, true, Timestamp::now()).await.unwrap();
    fixture.run(".rbit/workflows/ci.yml", "First change", RunEvent::Push).await;
    fixture.run(".rbit/workflows/nightly.yml", "Second <change>", RunEvent::Schedule).await;

    let runs = fixture.page("/alice/rocket/actions", &fixture.reader).await;
    let (first, second) = (runs.find("First change").unwrap(), runs.find("Second &#60;change&#62;").expect("titles are escaped"));
    assert!(second < first, "newest first");
    assert!(runs.contains("/alice/rocket/actions/workflows/ci.yml") && runs.contains("/alice/rocket/actions/workflows/nightly.yml"));

    let pushes = fixture.page("/alice/rocket/actions?event=push", &fixture.reader).await;
    assert!(pushes.contains("First change") && !pushes.contains("Second &#60;change"));
    let nightly = fixture.page("/alice/rocket/actions/workflows/nightly.yml", &fixture.reader).await;
    assert!(nightly.contains("Second &#60;change") && !nightly.contains("First change"));
    let failed = fixture.page("/alice/rocket/actions?status=failure", &fixture.reader).await;
    assert!(failed.contains("No matching runs"));
    let nobody = fixture.page("/alice/rocket/actions?actor=nobody", &fixture.reader).await;
    assert!(nobody.contains("No matching runs"));
    let (status, ..) = fixture.send(Method::GET, "/alice/rocket/actions/workflows/missing.yml", &fixture.reader, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn writers_cancel_and_run_again_and_readers_only_look() {
    let fixture = Fixture::new().await;
    fixture.database.actions_settings().set_enabled(fixture.repo_id().await, true, Timestamp::now()).await.unwrap();
    let run = fixture.run(".rbit/workflows/ci.yml", "Change", RunEvent::Push).await;
    let url = format!("/alice/rocket/actions/runs/{}", run.number);

    let page = fixture.page(&url, &fixture.owner).await;
    assert!(page.contains("needs build"));
    assert!(page.find(">build<").unwrap() < page.find(">test<").unwrap(), "jobs follow their needs");
    assert!(page.contains(&format!("{url}/cancel")));
    assert!(page.contains(r#"hx-trigger="every 3s""#), "a running run refreshes itself");
    let seen = fixture.page(&url, &fixture.reader).await;
    assert!(!seen.contains(&format!("{url}/cancel")), "readers get no buttons");

    let (status, ..) = fixture.send(Method::POST, &format!("{url}/cancel"), &fixture.reader, "").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, location) = fixture.send(Method::POST, &format!("{url}/cancel"), &fixture.owner, "").await;
    assert!(status.is_redirection());
    assert_eq!(location.as_deref(), Some(url.as_str()));
    let page = fixture.page(&url, &fixture.owner).await;
    assert!(!page.contains(r#"hx-trigger="every 3s""#), "a finished run stays still");
    assert!(page.contains(&format!("{url}/rerun")) && page.contains(&format!("{url}/rerun-failed")));

    let (status, ..) = fixture.send(Method::POST, &format!("{url}/rerun-failed"), &fixture.owner, "").await;
    assert!(status.is_redirection());
    let again = fixture.services.actions.as_ref().unwrap();
    let repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "rocket").await.unwrap();
    assert_eq!(again.run_detail(&repo, run.number).await.unwrap().run.attempt, 2);

    let (status, ..) = fixture.send(Method::GET, "/alice/rocket/actions/runs/999", &fixture.reader, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, ..) = fixture.send(Method::GET, "/alice/rocket/actions/runs/nope", &fixture.reader, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
