//! The job page's live tail (`…/logs/live`): only new lines of the running step, from the
//! page's offset on, within the step budget, for readers of the repository only.
use super::*;
use std::fmt::Write as _;

async fn live(fixture: &Fixture, path: &str, cookie: bool) -> (StatusCode, serde_json::Value) {
    let mut request = Request::builder().uri(path).header(header::HOST, "localhost:3000");
    if cookie {
        let token = fixture.services.accounts.start_session(&fixture.alice).await.unwrap();
        request = request.header(header::COOKIE, format!("rbit_session={}", token.encode()));
    }
    let response = fixture.router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    if status == StatusCode::OK {
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

#[tokio::test]
async fn the_live_tail_sends_only_new_lines_of_the_running_step() {
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let mut build = template("build", &["linux"], &[]);
    build.steps.push(build.steps[0].clone());
    let run = seed(&fixture, &[payload(&build)]).await;
    let job = fixture.database.actions_runs().jobs(run.id).await.unwrap().remove(0);
    let job_url = format!("/alice/rocket/actions/runs/{}/jobs/{}", run.number, job.id.0);
    let url = |from: u64| format!("{job_url}/logs/live?from={from}");

    let (status, queued) = live(&fixture, &url(0), true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((queued["active"].as_bool(), queued["target"].is_null(), queued["html"].as_str()), (Some(true), true, Some("")));

    let task = spec(&poll(&fixture, &credential, Some("live")).await.1).task_id.0;
    logs(&fixture, &credential, task, 0, "first step\n##[group]Run make\nmake <all>\n").await;
    state(
        &fixture,
        &credential,
        task,
        &json!({"steps": [step(0, "success", 0, 1), step(1, "running", 1, 0)], "outputs": {}, "result": null}),
    )
    .await;
    let (_, _, page) = get(&fixture, &job_url, false).await;
    let (_, answer) = live(&fixture, &url(0), true).await;
    assert!(page.contains(&format!(r#"data-live-state="{}""#, answer["state"].as_str().unwrap())), "the page and the tail agree");
    assert!(page.contains(r#"data-next="3""#), "the running step's console says where its tail goes on");
    assert_eq!(answer["target"], "1");
    let html = answer["html"].as_str().unwrap();
    assert!(!html.contains("first step"), "never lines before the running step");
    assert!(html.contains(r#"id="step-1-line-1""#) && html.contains("make &lt;all&gt;") && html.contains("data-unended"), "{html}");
    assert_eq!(answer["next"], 3);

    let (_, nothing) = live(&fixture, &url(3), true).await;
    assert_eq!((nothing["html"].as_str(), nothing["next"].as_u64()), (Some(""), Some(3)), "nothing new");
    logs(&fixture, &credential, task, 3, "built\n##[endgroup]\n##[error]it broke\n").await;
    let (_, more) = live(&fixture, &url(3), true).await;
    let html = more["html"].as_str().unwrap();
    assert!(html.starts_with(r#"<div class="log-line" id="step-1-line-3""#), "numbers go on from the offset: {html}");
    assert!(html.contains("data-endgroup") && html.contains(r#"<span class="log-label">Error: </span>it broke"#));
    assert_eq!(more["next"], 6);

    // Bounds: one answer holds at most the job page's step budget, and the page asks again.
    let long = (0..1500).fold(String::new(), |mut text, number| {
        writeln!(text, "line {number}").unwrap();
        text
    });
    logs(&fixture, &credential, task, 6, &long).await;
    let (_, first) = live(&fixture, &url(6), true).await;
    assert_eq!(first["html"].as_str().unwrap().matches("class=\"log-line\"").count(), 1000);
    assert_eq!(first["next"], 1006);
    let (_, rest) = live(&fixture, &url(1006), true).await;
    assert_eq!(rest["html"].as_str().unwrap().matches("class=\"log-line\"").count(), 500);

    // Readers only: the private repository's tail is not found without a session.
    let (status, _) = live(&fixture, &url(0), false).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = live(&fixture, &format!("/alice/rocket/actions/runs/{}/jobs/{}/logs/live", run.number + 1, job.id.0), true).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "the job of another run number");

    // A step change shows in the state, and a finished job is no longer active.
    state(
        &fixture,
        &credential,
        task,
        &json!({"steps": [step(0, "success", 0, 1), step(1, "failure", 1, 1505)], "outputs": {}, "result": "failure"}),
    )
    .await;
    let (_, done) = live(&fixture, &url(1506), true).await;
    assert_ne!(done["state"], answer["state"]);
    assert_eq!((done["active"].as_bool(), done["target"].is_null()), (Some(false), true));
}
