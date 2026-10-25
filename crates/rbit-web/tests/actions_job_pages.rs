//! Actions workstream U: the job page with its steps and logs, a step's own log page and
//! the raw log, for a job a runner really ran over the protocol.

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use serde_json::json;
use tower::ServiceExt;

use crate::{
    actions_runner::Fixture,
    actions_runner_tasks::{logs, payload, poll, seed, spec, state, template},
};

/// GET `path` as alice, the owner; returns the status, the headers and the body (gzipped
/// bodies decoded).
async fn get(fixture: &Fixture, path: &str, accept_gzip: bool) -> (StatusCode, axum::http::HeaderMap, String) {
    let token = fixture.services.accounts.start_session(&fixture.alice).await.unwrap();
    let mut request = Request::builder()
        .uri(path)
        .header(header::COOKIE, format!("rbit_session={}", token.encode()))
        .header(header::HOST, "localhost:3000");
    if accept_gzip {
        request = request.header(header::ACCEPT_ENCODING, "gzip");
    }
    let response = fixture.router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
    let (status, headers) = (response.status(), response.headers().clone());
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = if headers.get(header::CONTENT_ENCODING).is_some_and(|value| value == "gzip") {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut flate2::read::GzDecoder::new(&bytes[..]), &mut text).unwrap();
        text
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    (status, headers, body)
}

fn step(index: u32, status: &str, log_index: u64, log_length: u64) -> serde_json::Value {
    json!({"index": index, "status": status, "started_at": 1, "stopped_at": if status == "running" { None } else { Some(2) },
           "log_index": log_index, "log_length": log_length})
}

#[tokio::test]
async fn a_job_shows_its_steps_and_logs_and_serves_the_raw_log() {
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let mut build = template("build", &["linux"], &[]);
    build.steps.push(build.steps[0].clone());
    let run = seed(&fixture, &[payload(&build)]).await;
    let job = fixture.database.actions_runs().jobs(run.id).await.unwrap().remove(0);
    let job_url = format!("/alice/rocket/actions/runs/{}/jobs/{}", run.number, job.id.0);

    let (status, _, page) = get(&fixture, &job_url, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Not started yet") && page.contains(r#"hx-trigger="every 3s""#));
    let (status, ..) = get(&fixture, &format!("{job_url}/logs/raw"), false).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no log before a runner took the job");

    let task = spec(&poll(&fixture, &credential, Some("p")).await.1).task_id.0;
    logs(&fixture, &credential, task, 0, "\u{1b}[32mcompiling\u{1b}[0m <crate>\n##[error]it broke\nsecond step\n").await;
    let body = json!({"steps": [step(0, "failure", 0, 2), step(1, "running", 2, 0)], "outputs": {}, "result": null});
    state(&fixture, &credential, task, &body).await;

    let (_, _, page) = get(&fixture, &job_url, false).await;
    assert!(page.contains("compiling &lt;crate&gt;"), "escaped, without ANSI colours");
    assert!(!page.contains('\u{1b}'));
    assert!(page.contains(r#"<span class="text-danger">Error: it broke</span>"#));
    assert!(page.contains("second step"), "the running step reaches to the end of the log");
    assert_eq!(page.matches("<details open").count(), 1, "only the running step is open");
    assert!(page.contains(&format!("{job_url}/logs/raw")));

    let (status, _, step_page) = get(&fixture, &format!("{job_url}/logs?step=0"), false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(step_page.contains("compiling &lt;crate&gt;") && !step_page.contains("second step"));
    let (status, ..) = get(&fixture, &format!("{job_url}/logs?step=7"), false).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, headers, raw) = get(&fixture, &format!("{job_url}/logs/raw"), false).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "text/plain; charset=utf-8");
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert!(headers[header::CONTENT_DISPOSITION].to_str().unwrap().starts_with("inline;"));
    assert!(raw.starts_with("\u{1b}[32mcompiling"), "the raw log is the log as written");

    // Once finished, the log is stored gzipped and served as is to clients that take it.
    let body = json!({"steps": [step(0, "failure", 0, 2), step(1, "success", 2, 1)], "outputs": {}, "result": "failure"});
    state(&fixture, &credential, task, &body).await;
    let (_, headers, gzipped) = get(&fixture, &format!("{job_url}/logs/raw"), true).await;
    assert_eq!(headers.get(header::CONTENT_ENCODING).map(|value| value.to_str().unwrap()), Some("gzip"));
    let (_, headers, plain) = get(&fixture, &format!("{job_url}/logs/raw"), false).await;
    assert!(headers.get(header::CONTENT_ENCODING).is_none());
    assert_eq!(gzipped, plain);
    assert!(plain.ends_with("second step\n"));
    let (_, _, page) = get(&fixture, &job_url, false).await;
    assert!(!page.contains(r#"hx-trigger="every 3s""#), "a finished job stays still");
    assert!(page.contains("<details open"), "the failed step is open");

    let (status, ..) = get(&fixture, &format!("/alice/rocket/actions/runs/{}/jobs/{}", run.number + 1, job.id.0), false).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "the job of another run number");
}
