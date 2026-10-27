//! The REST API of rbit Actions beyond runs, workflows and settings (`docs/design/actions.md`
//! §4.10): a job's raw log, artifacts, and runners with their registration tokens.

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use rbit_core::{TokenScope, User};
use rbit_services::CreateAccessToken;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{
    actions_artifacts::{Answer, upload},
    actions_runner::Fixture,
    actions_runner_tasks::{logs, payload, poll, seed, spec, state, template},
};

async fn token(fixture: &Fixture, user: &User, scope: TokenScope) -> String {
    let input = CreateAccessToken { name: format!("{scope:?}"), scope, expires_in_days: None };
    fixture.services.tokens.create(user, input).await.unwrap().1
}

async fn api(fixture: &Fixture, method: Method, path: &str, token: &str) -> Answer {
    let request = Request::builder()
        .method(method)
        .uri(format!("/api/v1{path}"))
        .header(header::HOST, "git.example.com")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = fixture.router.clone().oneshot(request).await.unwrap();
    let (status, headers) = (response.status(), response.headers().clone());
    Answer { status, headers, body: to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec() }
}

#[tokio::test]
async fn job_logs_and_artifacts_are_read_downloaded_and_deleted() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let task = spec(&poll(&fixture, &credential, Some("a")).await.1).task_id.0;
    let (read, write) = (token(&fixture, &fixture.alice, TokenScope::Read).await, token(&fixture, &fixture.alice, TokenScope::Write).await);
    let actions = "/repos/alice/rocket/actions";
    let jobs = api(&fixture, Method::GET, &format!("{actions}/runs/{}/jobs", run.id.0), &read).await.json();
    let job = jobs["jobs"][0]["id"].as_i64().unwrap();

    // The raw log, while the task runs and once it is final (stored gzipped).
    assert_eq!(logs(&fixture, &credential, task, 0, "compiling\nlinking\n").await.0, StatusCode::OK);
    let log = api(&fixture, Method::GET, &format!("{actions}/jobs/{job}/logs"), &read).await;
    assert_eq!(log.status, StatusCode::OK);
    assert_eq!(log.headers[header::CONTENT_TYPE], "text/plain; charset=utf-8");
    assert!(String::from_utf8_lossy(&log.body).contains("compiling\nlinking"), "{:?}", String::from_utf8_lossy(&log.body));
    assert_eq!(api(&fixture, Method::GET, &format!("{actions}/jobs/{}/logs", job + 1000), &read).await.status, StatusCode::NOT_FOUND);

    // Artifacts: the run's, the repository's (by name), one, its file, and deleting it.
    assert_eq!(upload(&fixture, &credential, task, "dist", "", b"tar.gz bytes").await.status, StatusCode::CREATED);
    assert_eq!(upload(&fixture, &credential, task, "coverage", "", b"more bytes").await.status, StatusCode::CREATED);
    let listed = api(&fixture, Method::GET, &format!("{actions}/runs/{}/artifacts", run.id.0), &read).await.json();
    assert_eq!(listed["total_count"], 2);
    let names: Vec<_> = listed["artifacts"].as_array().unwrap().iter().map(|artifact| artifact["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["coverage", "dist"], "by name, as the run page");
    let all = api(&fixture, Method::GET, &format!("{actions}/artifacts?name=dist"), &read).await;
    assert_eq!(all.headers["x-total-count"], "1");
    let dist = all.json()["artifacts"][0].clone();
    assert_eq!((dist["size_in_bytes"].as_u64(), dist["expired"].as_bool()), (Some(12), Some(false)));
    assert!(dist["digest"].as_str().unwrap().starts_with("sha256:"));
    assert_eq!(dist["workflow_run"]["id"], run.id.0);
    let url = dist["url"].as_str().unwrap().strip_prefix("https://git.example.com/api/v1").unwrap().to_owned();
    assert_eq!(api(&fixture, Method::GET, &url, &read).await.json()["name"], "dist");
    assert_eq!(dist["archive_download_url"].as_str().unwrap(), format!("https://git.example.com/api/v1{url}/tar.gz"));
    let file = api(&fixture, Method::GET, &format!("{url}/tar.gz"), &read).await;
    assert_eq!((file.status, file.body.as_slice()), (StatusCode::OK, b"tar.gz bytes".as_slice()));
    assert_eq!(file.headers[header::CONTENT_DISPOSITION], "attachment; filename=\"dist.tar.gz\"");
    assert_eq!(api(&fixture, Method::GET, &format!("{url}/zip"), &read).await.status, StatusCode::NOT_FOUND, "tar.gz only");
    assert_eq!(api(&fixture, Method::DELETE, &url, &read).await.status, StatusCode::FORBIDDEN, "deleting needs the write scope");
    assert_eq!(api(&fixture, Method::DELETE, &url, &write).await.status, StatusCode::NO_CONTENT);
    assert_eq!(api(&fixture, Method::GET, &url, &read).await.status, StatusCode::NOT_FOUND);
    assert_eq!(api(&fixture, Method::GET, &format!("{url}/tar.gz"), &read).await.status, StatusCode::NOT_FOUND, "the file went too");
    assert_eq!(api(&fixture, Method::GET, &format!("{actions}/artifacts"), &read).await.json()["total_count"], 1);

    // Someone who cannot read the private repository learns nothing.
    let bob = fixture
        .services
        .accounts
        .create_user(
            rbit_services::SignUp { username: "bob".into(), email: "bob@example.com".into(), password: "correct horse battery".into() },
            Some(false),
        )
        .await
        .unwrap();
    let bobs = token(&fixture, &bob, TokenScope::Admin).await;
    for path in [format!("{actions}/jobs/{job}/logs"), format!("{actions}/artifacts"), format!("{actions}/runs/{}/artifacts", run.id.0)] {
        assert_eq!(api(&fixture, Method::GET, &path, &bobs).await.status, StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn runners_and_registration_tokens_need_admin_rights() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let credential = fixture.runner(&["linux"]).await;
    let (write, admin) =
        (token(&fixture, &fixture.alice, TokenScope::Write).await, token(&fixture, &fixture.alice, TokenScope::Admin).await);
    let runners = "/repos/alice/rocket/actions/runners";
    assert_eq!(api(&fixture, Method::GET, runners, &write).await.status, StatusCode::FORBIDDEN, "the admin scope");

    let listed = api(&fixture, Method::GET, runners, &admin).await.json();
    assert_eq!(listed["total_count"], 1);
    let runner = &listed["runners"][0];
    assert_eq!((runner["name"].as_str(), runner["os"].as_str(), runner["busy"].as_bool()), (Some("build-1"), Some("linux"), Some(false)));
    assert_eq!(runner["labels"], json!([{ "name": "linux", "type": "custom" }]));
    let id = runner["id"].as_i64().unwrap();

    // Busy while it runs a task.
    seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let task = spec(&poll(&fixture, &credential, Some("r")).await.1).task_id.0;
    let shown = api(&fixture, Method::GET, &format!("{runners}/{id}"), &admin).await.json();
    assert_eq!((shown["status"].as_str(), shown["busy"].as_bool()), (Some("online"), Some(true)), "{shown}");
    let done = json!({"steps": [], "outputs": {}, "result": "success"});
    assert_eq!(state(&fixture, &credential, task, &done).await.0, StatusCode::OK);
    assert_eq!(api(&fixture, Method::GET, &format!("{runners}/{id}"), &admin).await.json()["busy"], false);

    // A registration token, shown once and never cached.
    let issued = api(&fixture, Method::POST, &format!("{runners}/registration-token"), &admin).await;
    assert_eq!(issued.status, StatusCode::CREATED);
    assert_eq!(issued.headers[header::CACHE_CONTROL], "no-store");
    let issued = issued.json();
    assert!(issued["token"].as_str().unwrap().starts_with("rbrg_"), "{issued}");
    assert!(issued["expires_at"].is_string());
    assert_eq!(api(&fixture, Method::POST, &format!("{runners}/registration-token"), &write).await.status, StatusCode::FORBIDDEN);

    // The caller's own runners are not the repository's.
    assert_eq!(api(&fixture, Method::GET, "/user/actions/runners", &admin).await.json()["total_count"], 0);
    assert_eq!(api(&fixture, Method::GET, &format!("/user/actions/runners/{id}"), &admin).await.status, StatusCode::NOT_FOUND);

    // Removing it ends its credential.
    assert_eq!(api(&fixture, Method::DELETE, &format!("{runners}/{id}"), &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(api(&fixture, Method::GET, &format!("{runners}/{id}"), &admin).await.status, StatusCode::NOT_FOUND);
    assert_eq!(poll(&fixture, &credential, Some("gone")).await.0, StatusCode::UNAUTHORIZED);
    let _: Value = api(&fixture, Method::GET, runners, &admin).await.json();
}

// Actions: metrics.
#[tokio::test]
async fn metrics_count_each_transition_once() {
    // This test's own recorder: the global one is shared by every test of the binary.
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let handle = recorder.handle();
    let _local = metrics::set_default_local_recorder(&recorder);

    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let credential = fixture.runner(&["linux"]).await;
    seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let task = spec(&poll(&fixture, &credential, Some("m")).await.1).task_id.0;
    assert_eq!(logs(&fixture, &credential, task, 0, "hello\n").await.0, StatusCode::OK);
    let done = json!({"steps": [], "outputs": {}, "result": "success"});
    assert_eq!(state(&fixture, &credential, task, &done).await.0, StatusCode::OK);
    assert_eq!(state(&fixture, &credential, task, &done).await.0, StatusCode::OK, "a resent final call");
    assert_eq!(poll(&fixture, "rbrn_not-a-credential", Some("x")).await.0, StatusCode::UNAUTHORIZED);
    fixture.actions().record_gauges(rbit_core::Timestamp::now()).await.unwrap();

    let text = handle.render();
    let value = |series: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(series).and_then(|rest| rest.strip_prefix(' ')).and_then(|value| value.parse::<f64>().ok()))
    };
    assert_eq!(value(r#"rbit_actions_jobs_total{outcome="success"}"#), Some(1.0), "{text}");
    assert_eq!(value(r#"rbit_actions_runs_total{event="push",outcome="success"}"#), Some(1.0), "once, though reported twice");
    assert_eq!(value("rbit_actions_job_queue_seconds_count"), Some(1.0));
    assert_eq!(value("rbit_actions_job_duration_seconds_count"), Some(1.0));
    assert!(value("rbit_actions_log_bytes_total").is_some_and(|bytes| bytes >= 6.0), "{text}");
    assert_eq!(value(r#"rbit_actions_rpc_total{method="register",code="ok"}"#), Some(1.0));
    assert_eq!(value(r#"rbit_actions_rpc_total{method="task_state",code="ok"}"#), Some(2.0));
    assert_eq!(value(r#"rbit_actions_rpc_total{method="poll",code="unauthenticated"}"#), Some(1.0));
    assert_eq!(value("rbit_actions_runners_online"), Some(1.0));
    assert_eq!(value("rbit_actions_jobs_waiting"), Some(0.0));
}

// Actions: step summaries.
#[tokio::test]
async fn step_summaries_are_kept_once_and_shown_sanitised_on_the_run_page() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let task = spec(&poll(&fixture, &credential, Some("s")).await.1).task_id.0;
    let first = json!({
        "steps": [], "outputs": {}, "result": null,
        "summaries": {"0": "## Benchmarks\n\n| case | ms |\n|---|---|\n| parse | 12 |\n\n<script>alert(1)</script>", "9": "no such step"},
    });
    assert_eq!(state(&fixture, &credential, task, &first).await.0, StatusCode::OK);
    let again = json!({"steps": [], "outputs": {}, "result": "success", "summaries": {"0": "a later, different summary"}});
    assert_eq!(state(&fixture, &credential, task, &again).await.0, StatusCode::OK);
    let oversized = json!({"steps": [], "outputs": {}, "result": null, "summaries": {"1": "x".repeat(64 * 1024 + 1)}});
    assert_eq!(state(&fixture, &credential, task, &oversized).await.0, StatusCode::BAD_REQUEST, "past the protocol's limit");

    let token = fixture.services.accounts.start_session(&fixture.alice).await.unwrap();
    let request = Request::builder()
        .uri(format!("/alice/rocket/actions/runs/{}", run.number))
        .header(header::HOST, "localhost:3000")
        .header(header::COOKIE, format!("rbit_session={}", token.encode()))
        .body(Body::empty())
        .unwrap();
    let response = fixture.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let page = String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
    assert!(page.contains("build summary") && page.contains("Benchmarks") && page.contains("<table>"), "rendered Markdown: {page}");
    assert!(!page.contains("<script>alert(1)"), "sanitised");
    assert!(!page.contains("a later, different summary"), "the first summary of a step is kept");
    assert!(!page.contains("no such step"), "only the task's own steps");
}
