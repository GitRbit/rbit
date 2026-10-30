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
            NewJob {
                key: "build",
                name: "build",
                matrix: None,
                needs: &[],
                runs_on: &labels,
                permissions: None,
                payload: b"{}",
                condition: false,
                max_parallel: None,
                group: None,
                environment: None,
            },
            NewJob {
                key: "test",
                name: "test",
                matrix: None,
                needs: &needs,
                runs_on: &labels,
                permissions: None,
                payload: b"{}",
                condition: false,
                max_parallel: None,
                group: None,
                environment: None,
            },
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

    // Actions: workflow status badges.
    /// A run of `workflow` on `reference` that failed before any job (a workflow error).
    async fn failed_run(&self, workflow: &str, reference: &str, event: RunEvent) -> rbit_core::actions::Run {
        self.failed_run_in(self.repo_id().await, workflow, reference, event).await
    }

    async fn failed_run_in(&self, repo: rbit_core::RepoId, workflow: &str, reference: &str, event: RunEvent) -> rbit_core::actions::Run {
        let new = NewRun {
            repo,
            workflow_path: workflow,
            workflow_name: "Build & <test>",
            title: "broken",
            event,
            reference,
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
            jobs: &[],
            error: Some("a broken workflow"),
            now: Timestamp::now(),
        };
        self.database.actions_runs().create_run(&new).await.unwrap()
    }

    /// The status, headers and body of `path` for `cookie` (none when empty).
    async fn get(&self, path: &str, cookie: &str, if_none_match: Option<&str>) -> (StatusCode, axum::http::HeaderMap, String) {
        let mut request = Request::builder().uri(path).header(header::HOST, "localhost:3000");
        if !cookie.is_empty() {
            request = request.header(header::COOKIE, cookie);
        }
        if let Some(tag) = if_none_match {
            request = request.header(header::IF_NONE_MATCH, tag);
        }
        let response = self.router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        let (status, headers) = (response.status(), response.headers().clone());
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, headers, String::from_utf8_lossy(&body).into_owned())
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
    let list = fixture.page("/alice/rocket/actions?event=push", &fixture.reader).await;
    assert!(
        list.contains(r#"id="runs" hx-get="/alice/rocket/actions?event=push" hx-trigger="every 3s""#),
        "the list follows its runs, filtered"
    );
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
    let list = fixture.page("/alice/rocket/actions", &fixture.reader).await;
    assert!(list.contains(r#"id="runs" hx-get="/alice/rocket/actions" hx-trigger="every 10s""#), "the newest runs look for new ones");
    let older = fixture.page(&format!("/alice/rocket/actions?before={}", run.id.0 + 1), &fixture.reader).await;
    assert!(older.contains("Change") && !older.contains(r#"id="runs" hx-get"#), "an older page of finished runs stays still");

    let (status, ..) = fixture.send(Method::POST, &format!("{url}/rerun-failed"), &fixture.owner, "").await;
    assert!(status.is_redirection());
    let again = fixture.services.actions.snapshot().unwrap();
    let repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "rocket").await.unwrap();
    assert_eq!(again.run_detail(&repo, run.number).await.unwrap().run.attempt, 2);

    let (status, ..) = fixture.send(Method::GET, "/alice/rocket/actions/runs/999", &fixture.reader, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, ..) = fixture.send(Method::GET, "/alice/rocket/actions/runs/nope", &fixture.reader, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// Actions: dispatch form.
/// Commits `files` to `alice/rocket`'s `main`.
fn push(repo: &rbit_services::RepoContext, files: &[(&str, &str)]) {
    let work = repo.disk_path.with_extension("work");
    std::fs::create_dir_all(&work).unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main", "-c", "core.autocrlf=false"])
            .args(args)
            .current_dir(&work)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    };
    git(&["init", "-q"]);
    for (path, contents) in files {
        let file = work.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
    }
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "Workflows"]);
    git(&["push", "-q", repo.disk_path.to_str().unwrap(), "HEAD:refs/heads/main"]);
}

const RELEASE: &str = "name: Release
on:
  workflow_dispatch:
    inputs:
      version: {description: The version to tag, required: true}
      channel: {type: choice, options: [stable, beta], default: stable}
      dry-run: {type: boolean}
jobs:
  tag:
    runs-on: ubuntu-latest
    steps:
      - run: echo tagging
";

#[tokio::test]
async fn writers_run_dispatch_workflows_from_their_page() {
    let fixture = Fixture::new().await;
    fixture.database.actions_settings().set_enabled(fixture.repo_id().await, true, Timestamp::now()).await.unwrap();
    let repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "rocket").await.unwrap();
    let ci = "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: make\n";
    push(&repo, &[(".rbit/workflows/release.yml", RELEASE), (".rbit/workflows/ci.yml", ci)]);

    // Neither ran, yet both have a page; only the dispatch one offers the form, to writers.
    let list = fixture.page("/alice/rocket/actions", &fixture.owner).await;
    assert!(list.contains(r#"href="/alice/rocket/actions/workflows/release.yml""#) && list.contains("/actions/workflows/ci.yml"));
    let url = "/alice/rocket/actions/workflows/release.yml";
    let page = fixture.page(url, &fixture.owner).await;
    assert!(page.contains("Run workflow") && page.contains(r#"action="/alice/rocket/actions/workflows/release.yml/dispatch""#));
    assert!(page.contains(r#"name="inputs[version]""#) && page.contains("The version to tag"));
    assert!(page.contains(r#"<option value="stable" selected>stable</option>"#), "the default is chosen");
    assert!(page.contains(r#"type="checkbox" id="dispatch-input-2" name="inputs[dry-run]" value="true""#));
    assert!(page.contains(r#"id="dispatch-ref" name="ref" value="main""#), "the default branch");
    assert!(page.contains(">Release</h1>") && page.contains("<title>Release · Actions"), "the workflow's own name, not its file's");
    assert!(page.contains("use Run workflow above"), "no runs yet, and no \"add a workflow file\"");
    assert!(!fixture.page(url, &fixture.reader).await.contains("Run workflow"), "readers cannot run it");
    assert!(!fixture.page("/alice/rocket/actions/workflows/ci.yml", &fixture.owner).await.contains("Run workflow"));
    let unknown = fixture.page(&format!("{url}?ref=nope"), &fixture.owner).await;
    assert!(unknown.contains("the ref must be a branch or tag of this repository"), "said in the form");

    // What the workflow refuses comes back with the values given.
    let dispatch = format!("{url}/dispatch");
    let missing = "ref=main&inputs%5Bversion%5D=&inputs%5Bchannel%5D=beta&inputs%5Bdry-run%5D=false";
    let (status, body, _) = fixture.send(Method::POST, &dispatch, &fixture.owner, missing).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.contains("the input `version` is required") && body.contains(r#"<option value="beta" selected>beta</option>"#));
    let (status, ..) = fixture.send(Method::POST, &dispatch, &fixture.reader, missing).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A good one starts a run and shows it; a ticked box sends `true` after the hidden `false`.
    let good = "ref=main&inputs%5Bversion%5D=1.2.0&inputs%5Bchannel%5D=beta&inputs%5Bdry-run%5D=false&inputs%5Bdry-run%5D=true";
    let (status, _, location) = fixture.send(Method::POST, &dispatch, &fixture.owner, good).await;
    assert!(status.is_redirection(), "{status}");
    assert_eq!(location.as_deref(), Some("/alice/rocket/actions/runs/1"));
    let actions = fixture.services.actions.snapshot().unwrap();
    let detail = actions.run_detail(&repo, 1).await.unwrap();
    assert_eq!((detail.run.event, detail.run.title.as_str()), (RunEvent::WorkflowDispatch, "Release ran manually"));
    let (_, inputs) = fixture.database.actions_runs().event(detail.run.id).await.unwrap().unwrap();
    let inputs: serde_json::Value = serde_json::from_str(&inputs.unwrap()).unwrap();
    assert_eq!(inputs, serde_json::json!({"version": "1.2.0", "channel": "beta", "dry-run": true}));
}

// Actions: disabled workflows.
#[tokio::test]
async fn writers_disable_and_enable_a_workflow_from_its_page() {
    let fixture = Fixture::new().await;
    fixture.database.actions_settings().set_enabled(fixture.repo_id().await, true, Timestamp::now()).await.unwrap();
    let repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "rocket").await.unwrap();
    push(&repo, &[(".rbit/workflows/release.yml", RELEASE)]);
    let url = "/alice/rocket/actions/workflows/release.yml";
    let switch = format!("{url}/switch");

    let page = fixture.page(url, &fixture.owner).await;
    assert!(page.contains("Disable workflow") && page.contains(&format!(r#"action="{switch}""#)) && page.contains("Run workflow"));
    assert!(!fixture.page(url, &fixture.reader).await.contains("Disable workflow"), "readers cannot switch it");
    let (status, ..) = fixture.send(Method::POST, &switch, &fixture.reader, "disabled=true").await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _, location) = fixture.send(Method::POST, &switch, &fixture.owner, "disabled=true").await;
    assert!(status.is_redirection(), "{status}");
    assert_eq!(location.as_deref(), Some(url));
    let page = fixture.page(url, &fixture.owner).await;
    assert!(page.contains("This workflow is disabled") && page.contains("Enable workflow") && !page.contains("Run workflow"));
    assert!(fixture.page(url, &fixture.reader).await.contains("This workflow is disabled"), "everyone sees it");
    let actions = fixture.services.actions.snapshot().unwrap();
    let request = rbit_services::actions::dispatch::DispatchRequest {
        workflow: "release.yml".into(),
        reference: "main".into(),
        inputs: [("version".to_owned(), "1.0.0".to_owned())].into(),
    };
    let refused = actions.dispatch(&fixture.alice, &repo, &request).await.unwrap_err();
    assert!(refused.to_string().contains("is disabled"), "{refused}");

    let (status, ..) = fixture.send(Method::POST, &switch, &fixture.owner, "disabled=false").await;
    assert!(status.is_redirection(), "{status}");
    assert!(fixture.page(url, &fixture.owner).await.contains("Disable workflow"));
    assert_eq!(actions.dispatch(&fixture.alice, &repo, &request).await.unwrap().number, 1, "enabled, it runs again");
    let (status, ..) = fixture.send(Method::POST, "/alice/rocket/actions/workflows/nope.yml/switch", &fixture.owner, "disabled=true").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// Actions: workflow status badges.
#[tokio::test]
async fn workflow_badges_show_the_newest_finished_run_of_a_branch() {
    let fixture = Fixture::new().await;
    let badge = "/alice/rocket/actions/workflows/ci.yml/badge.svg";
    let (status, ..) = fixture.get(badge, "", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Actions is off");
    fixture.database.actions_settings().set_enabled(fixture.repo_id().await, true, Timestamp::now()).await.unwrap();
    let (status, ..) = fixture.get(badge, "", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no such workflow: no run, no file");

    // A run that has not finished says nothing yet.
    fixture.run(".rbit/workflows/ci.yml", "running", RunEvent::Push).await;
    let (status, headers, body) = fixture.get(badge, "", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(">no status</text>") && body.contains(">CI</text>"), "{body}");
    assert_eq!(headers[header::CONTENT_TYPE], "image/svg+xml; charset=utf-8");
    assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=60");
    assert!(headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap().starts_with("default-src 'none'"));

    // The newest finished run of the default branch, from any event unless one is asked.
    fixture.failed_run(".rbit/workflows/ci.yml", "refs/heads/main", RunEvent::Push).await;
    let (_, headers, body) = fixture.get(badge, "", None).await;
    assert!(body.contains(">failing</text>") && body.contains("Build &amp; &lt;test&gt;"), "names are escaped: {body}");
    let (status, _, empty) = fixture.get(badge, "", Some(headers[header::ETAG].to_str().unwrap())).await;
    assert_eq!((status, empty.as_str()), (StatusCode::NOT_MODIFIED, ""), "the ETag revalidates");
    let (_, _, body) = fixture.get(&format!("{badge}?event=schedule"), "", None).await;
    assert!(body.contains(">no status</text>"), "no scheduled run finished");
    let (_, _, body) = fixture.get(&format!("{badge}?event=nonsense"), "", None).await;
    assert!(body.contains(">no status</text>"));
    let (_, _, body) = fixture.get(&format!("{badge}?branch=feature"), "", None).await;
    assert!(body.contains(">no status</text>"), "another branch has no runs");
    fixture.failed_run(".rbit/workflows/ci.yml", "refs/heads/feature", RunEvent::Push).await;
    let (_, _, body) = fixture.get(&format!("{badge}?branch=feature&event=push"), "", None).await;
    assert!(body.contains(">failing</text>"));

    // A cancelled run is passed over.
    let cancelled = fixture.run(".rbit/workflows/ci.yml", "cancelled", RunEvent::Push).await;
    fixture.database.actions_runs().cancel_run(fixture.repo_id().await, cancelled.id, Timestamp::now()).await.unwrap();
    let (_, _, body) = fixture.get(badge, "", None).await;
    assert!(body.contains(">failing</text>"), "{body}");

    // Names that are no workflow file.
    for path in ["/alice/rocket/actions/workflows/..%2Fci.yml/badge.svg", "/alice/rocket/actions/workflows/.hidden/badge.svg"] {
        assert_eq!(fixture.get(path, "", None).await.0, StatusCode::NOT_FOUND, "{path}");
    }

    // Private: readers see it, cached only by their browser; everyone else gets the same 404
    // as a repository that does not exist.
    let input = CreateRepository { name: "vault".into(), description: String::new(), visibility: rbit_core::Visibility::Private };
    fixture.services.repositories.create(&fixture.alice, input).await.unwrap();
    let vault = fixture.database.repositories().find_by_path("alice", "vault").await.unwrap().unwrap().repo.id;
    fixture.database.actions_settings().set_enabled(vault, true, Timestamp::now()).await.unwrap();
    fixture.failed_run_in(vault, ".rbit/workflows/ci.yml", "refs/heads/main", RunEvent::Push).await;
    let badge = "/alice/vault/actions/workflows/ci.yml/badge.svg";
    let (status, headers, body) = fixture.get(badge, &fixture.owner, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(">failing</text>"));
    assert_eq!(headers[header::CACHE_CONTROL], "private, max-age=60");
    let (hidden, hidden_headers, hidden_body) = fixture.get(badge, "", None).await;
    let (outsider, ..) = fixture.get(badge, &fixture.reader, None).await;
    let (missing, missing_headers, missing_body) = fixture.get("/alice/nothing-here/actions/workflows/ci.yml/badge.svg", "", None).await;
    assert_eq!((hidden, outsider, missing), (StatusCode::NOT_FOUND, StatusCode::NOT_FOUND, StatusCode::NOT_FOUND));
    assert_eq!(hidden_headers[header::CONTENT_TYPE], missing_headers[header::CONTENT_TYPE], "the same page, not an image");
    assert!(hidden_body.contains("Page not found") && missing_body.contains("Page not found"));
    assert!(!hidden_body.contains("failing") && !hidden_body.contains("passing"), "nothing of the workflow shows");
}
