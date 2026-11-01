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
    actions_artifacts::{Answer, stored, tar_gz_files, upload, zip_files},
    actions_runner::Fixture,
    actions_runner_tasks::{logs, payload, poll, seed, spec, state, template},
};

pub async fn token(fixture: &Fixture, user: &User, scope: TokenScope) -> String {
    let input = CreateAccessToken { name: format!("{scope:?}"), scope, expires_in_days: None };
    fixture.services.tokens.create(user, input).await.unwrap().1
}

pub async fn api(fixture: &Fixture, method: Method, path: &str, token: &str) -> Answer {
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
    // Actions: run logs. The whole run's, as the zip `gh run view --log` reads.
    let zipped = api(&fixture, Method::GET, &format!("{actions}/runs/{}/logs", run.id.0), &read).await;
    assert_eq!((zipped.status, zipped.headers[header::CONTENT_TYPE].to_str().unwrap()), (StatusCode::OK, "application/zip"));
    let files = zip_files(&zipped.body);
    assert_eq!(files.len(), 1);
    let extension = std::path::Path::new(&files[0].0).extension().and_then(|extension| extension.to_str());
    assert!(files[0].0.starts_with("0_") && extension == Some("txt") && files[0].1.contains("compiling\nlinking"), "{files:?}");

    // Artifacts: the run's, the repository's (by name), one, its file, and deleting it.
    let dist = stored(&[("dist/app.zip", b"zipped"), ("dist/app.zip.sha256", b"sum")]);
    assert_eq!(upload(&fixture, &credential, task, "dist", "", &dist).await.status, StatusCode::CREATED);
    assert_eq!(upload(&fixture, &credential, task, "coverage", "", &stored(&[("lcov.info", b"TN:")])).await.status, StatusCode::CREATED);
    let listed = api(&fixture, Method::GET, &format!("{actions}/runs/{}/artifacts", run.id.0), &read).await.json();
    assert_eq!(listed["total_count"], 2);
    let names: Vec<_> = listed["artifacts"].as_array().unwrap().iter().map(|artifact| artifact["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["coverage", "dist"], "by name, as the run page");
    let all = api(&fixture, Method::GET, &format!("{actions}/artifacts?name=dist"), &read).await;
    assert_eq!(all.headers["x-total-count"], "1");
    let size = dist.len() as u64;
    let dist = all.json()["artifacts"][0].clone();
    assert_eq!((dist["size_in_bytes"].as_u64(), dist["expired"].as_bool()), (Some(size), Some(false)));
    assert!(dist["digest"].as_str().unwrap().starts_with("sha256:"));
    assert_eq!(dist["workflow_run"]["id"], run.id.0);
    let url = dist["url"].as_str().unwrap().strip_prefix("https://git.example.com/api/v1").unwrap().to_owned();
    assert_eq!(api(&fixture, Method::GET, &url, &read).await.json()["name"], "dist");
    assert_eq!(dist["archive_download_url"].as_str().unwrap(), format!("https://git.example.com/api/v1{url}/zip"), "GitHub's format");
    let files = [("dist/app.zip".to_owned(), "zipped".to_owned()), ("dist/app.zip.sha256".to_owned(), "sum".to_owned())];
    let file = api(&fixture, Method::GET, &format!("{url}/zip"), &read).await;
    assert_eq!((file.status, zip_files(&file.body)), (StatusCode::OK, files.to_vec()));
    assert_eq!(file.headers[header::CONTENT_TYPE], "application/zip");
    assert_eq!(file.headers[header::CONTENT_DISPOSITION], "attachment; filename=\"dist.zip\"");
    let file = api(&fixture, Method::GET, &format!("{url}/tar.gz"), &read).await;
    assert_eq!((file.status, tar_gz_files(&file.body)), (StatusCode::OK, files.to_vec()));
    assert_eq!(file.headers[header::CONTENT_DISPOSITION], "attachment; filename=\"dist.tar.gz\"");
    assert_eq!(api(&fixture, Method::GET, &format!("{url}/7z"), &read).await.status, StatusCode::NOT_FOUND, "zip and tar.gz only");
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
        .uri(format!("/alice/rocket/-/plugins/rbit.actions/run?run={}", run.number))
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

// Actions: step summaries.
/// Tasks are numbered as runners claim jobs, so a later job may hold the earlier task; each
/// job still shows its own summary.
#[tokio::test]
async fn step_summaries_follow_their_jobs_whatever_order_they_were_claimed_in() {
    use tracing::instrument::WithSubscriber;

    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let (linux, mac) = (fixture.runner(&["linux"]).await, fixture.runner(&["mac"]).await);
    let run = seed(&fixture, &[payload(&template("first", &["mac"], &[])), payload(&template("second", &["linux"], &[]))]).await;
    let second = spec(&poll(&fixture, &linux, Some("l")).await.1).task_id.0;
    let first = spec(&poll(&fixture, &mac, Some("m")).await.1).task_id.0;
    assert!(second < first, "the second job holds the earlier task");
    for (credential, task, text) in [(&linux, second, "summary of the second job"), (&mac, first, "summary of the first job")] {
        let body = json!({"steps": [], "outputs": {}, "result": null, "summaries": {"0": text}});
        assert_eq!(state(&fixture, credential, task, &body).await.0, StatusCode::OK);
    }

    let token = fixture.services.accounts.start_session(&fixture.alice).await.unwrap();
    let request = Request::builder()
        .uri(format!("/alice/rocket/-/plugins/rbit.actions/run?run={}", run.number))
        .header(header::HOST, "localhost:3000")
        .header(header::COOKIE, format!("rbit_session={}", token.encode()))
        .body(Body::empty())
        .unwrap();
    // A 503 deliberately has a generic body. Keep the existing boundary's
    // PageError diagnostics local to this request, without owning the global
    // subscriber used by SQL query instrumentation in the same test binary.
    let diagnostics = tracing_subscriber::fmt().with_env_filter("rbit_web::routes::plugin_pages=debug").with_test_writer().finish();
    let response = fixture.router.clone().oneshot(request).with_subscriber(diagnostics).await.unwrap();
    let status = response.status();
    let page = String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
    assert_eq!(status, StatusCode::OK, "run page response: {page}");
    assert!(page.contains("summary of the first job") && page.contains("summary of the second job"), "{page}");
}

// Actions: disabled workflows.
#[tokio::test]
async fn workflows_are_shown_disabled_enabled_and_their_runs_listed() {
    let fixture = Fixture::new().await;
    seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let (read, write) = (token(&fixture, &fixture.alice, TokenScope::Read).await, token(&fixture, &fixture.alice, TokenScope::Write).await);
    let workflows = "/repos/alice/rocket/actions/workflows";

    let listed = api(&fixture, Method::GET, workflows, &read).await.json();
    assert_eq!((listed["workflows"][0]["id"].as_str(), listed["workflows"][0]["state"].as_str()), (Some("ci.yml"), Some("active")));
    let shown = api(&fixture, Method::GET, &format!("{workflows}/ci.yml"), &read).await.json();
    assert_eq!(shown["path"], ".rbit/workflows/ci.yml");
    let by_path = api(&fixture, Method::GET, &format!("{workflows}/.rbit%2Fworkflows%2Fci.yml"), &read).await;
    assert_eq!(by_path.json()["id"], "ci.yml", "by path too");
    let runs = api(&fixture, Method::GET, &format!("{workflows}/ci.yml/runs"), &read).await.json();
    assert_eq!(runs["total_count"], 1);

    let disable = format!("{workflows}/ci.yml/disable");
    assert_eq!(api(&fixture, Method::PUT, &disable, &read).await.status, StatusCode::FORBIDDEN, "needs the write scope");
    assert_eq!(api(&fixture, Method::PUT, &disable, &write).await.status, StatusCode::NO_CONTENT);
    assert_eq!(api(&fixture, Method::GET, &format!("{workflows}/ci.yml"), &read).await.json()["state"], "disabled_manually");
    assert_eq!(api(&fixture, Method::PUT, &format!("{workflows}/ci.yml/enable"), &write).await.status, StatusCode::NO_CONTENT);
    assert_eq!(api(&fixture, Method::GET, workflows, &read).await.json()["workflows"][0]["state"], "active");
    assert_eq!(api(&fixture, Method::PUT, &format!("{workflows}/nope.yml/disable"), &write).await.status, StatusCode::NOT_FOUND);
}

/// Markdown expansion cannot inflate the run document or hide the original
/// summary: one step remains readable as escaped source, with current-task navigation.
#[tokio::test]
async fn native_actions_step_summary_documents_bound_amplification_and_keep_full_step_navigation() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let job = fixture.database.actions_runs().jobs(run.id).await.unwrap().remove(0);
    let task = spec(&poll(&fixture, &credential, Some("summary-bounds")).await.1).task_id.0;
    let source = format!(
        "## Full summary\n\n{}|\n{}|\n{}\n<script>summary_untrusted()</script>\nFULL_SOURCE_TAIL_MARKER",
        "|x".repeat(100),
        "|-".repeat(100),
        format!("{}\n", "|".repeat(101)).repeat(620),
    );
    assert!(source.len() <= 64 * 1024);
    assert!(
        rbit_web::render::render_markdown_bounded(&source, 512 * 1024).is_none(),
        "fixture must amplify beyond the full-page HTML budget"
    );
    let body = json!({"steps": [], "outputs": {}, "result": "success", "summaries": {"0": source, "1": "SECOND_STEP_COMPLETE"}});
    assert_eq!(state(&fixture, &credential, task, &body).await.0, StatusCode::OK);
    let cookie = format!("rbit_session={}", fixture.services.accounts.start_session(&fixture.alice).await.unwrap().encode());
    let base = "/alice/rocket/-/plugins/rbit.actions";
    let (status, page) = summary_document(&fixture, &cookie, &format!("{base}/run?run={}", run.number)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.len() < 256 * 1024, "run preview is bounded");
    assert!(page.contains(&format!("{base}/summary?run={}", run.number)));
    let summary = format!("{base}/summary?run={}&job={}", run.number, job.id.0);
    let (status, page) = summary_document(&fixture, &cookie, &format!("{summary}&step=0")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("FULL_SOURCE_TAIL_MARKER"), "the complete source remains accessible");
    assert!(page.contains("showing its Markdown source"));
    assert!(!page.contains("<script>summary_untrusted()"));
    assert!(page.contains("step=1"));
    let (status, page) = summary_document(&fixture, &cookie, &format!("{summary}&step=1")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("SECOND_STEP_COMPLETE") && page.contains("step=0"));
    assert!(!page.contains("FULL_SOURCE_TAIL_MARKER"));

    let other = fixture
        .services
        .repositories
        .create(
            &fixture.alice,
            rbit_services::CreateRepository {
                name: "other".into(),
                description: String::new(),
                visibility: rbit_core::Visibility::Private,
            },
        )
        .await
        .unwrap();
    fixture.database.actions_settings().set_enabled(other.repo.id, true, rbit_core::Timestamp::now()).await.unwrap();
    let path = format!("/alice/other/-/plugins/rbit.actions/summary?run={}&job={}", run.number, job.id.0);
    assert_eq!(summary_document(&fixture, &cookie, &path).await.0, StatusCode::NOT_FOUND);
    fixture.actions().rerun(&fixture.alice, &fixture.repo().await, run.id, rbit_storage::sqlite::actions_runs::Rerun::All).await.unwrap();
    assert_eq!(
        summary_document(&fixture, &cookie, &summary).await.0,
        StatusCode::NOT_FOUND,
        "old-attempt summaries do not attach to a new task"
    );
}

async fn summary_document(fixture: &Fixture, cookie: &str, path: &str) -> (StatusCode, String) {
    let request =
        Request::builder().uri(path).header(header::HOST, "git.example.com").header(header::COOKIE, cookie).body(Body::empty()).unwrap();
    let response = fixture.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    if status == StatusCode::OK {
        assert!(response.headers()[header::CACHE_CONTROL].to_str().unwrap().contains("no-store"));
    }
    let bytes = to_bytes(response.into_body(), 3 * 1024 * 1024).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}
