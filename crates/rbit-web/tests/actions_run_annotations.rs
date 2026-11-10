//! Actions annotations: the error, warning and notice lines a job's log carries are kept as
//! the runner appends them and listed on the run page, linking to their job at the line.

use axum::{
    body::{Body, to_bytes},
    http::{Request, header},
};
use serde_json::json;
use tower::ServiceExt;

use crate::{
    actions_runner::Fixture,
    actions_runner_tasks::{logs, payload, poll, seed, spec, state, template},
};

async fn page(fixture: &Fixture, path: &str) -> String {
    let token = fixture.services.accounts.start_session(&fixture.alice).await.unwrap();
    let request = Request::builder()
        .uri(path)
        .header(header::COOKIE, format!("rbit_session={}", token.encode()))
        .header(header::HOST, "localhost:3000")
        .body(Body::empty())
        .unwrap();
    let response = fixture.router.clone().oneshot(request).await.unwrap();
    String::from_utf8_lossy(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).into_owned()
}

fn step(index: u32, status: &str, log_index: u64, log_length: u64) -> serde_json::Value {
    json!({"index": index, "status": status, "started_at": 1, "stopped_at": 2, "log_index": log_index, "log_length": log_length})
}

#[tokio::test]
async fn the_run_page_lists_annotations_and_failed_steps_linking_to_their_lines() {
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let job = fixture.database.actions_runs().jobs(run.id).await.unwrap().remove(0);
    let run_url = format!("/alice/rocket/actions/runs/{}", run.number);
    let job_url = format!("{run_url}/jobs/{}", job.id.0);
    let task = spec(&poll(&fixture, &credential, Some("a")).await.1).task_id.0;

    let lines = "set up\n##[error]mismatched <types> (file=src/a.rs,line=3,title=E0308)\n##[warning]careful\ntests\n";
    logs(&fixture, &credential, task, 0, lines).await;
    // A task keeps the first annotations only, however many its log carries.
    let many = (0..80).map(|index| format!("##[notice]n{index}")).collect::<Vec<_>>().join("\n") + "\n";
    logs(&fixture, &credential, task, 4, &many).await;
    let kept = fixture.database.actions_runs().annotations_of_tasks(&[rbit_core::actions::TaskId(task)]).await.unwrap();
    assert_eq!(kept.len(), 50);
    assert_eq!((kept[0].line, kept[0].level.as_str()), (1, "error"));
    let body = json!({"steps": [step(0, "failure", 1, 2), step(1, "failure", 3, 1)], "outputs": {}, "result": "failure"});
    state(&fixture, &credential, task, &body).await;

    let page = page(&fixture, &run_url).await;
    assert!(page.contains(r#"id="run-annotations""#) && page.contains("Annotations"), "{page}");
    assert!(page.contains("2 errors") && page.contains("1 warning") && page.contains("48 notices"));
    assert!(page.contains(&format!(r#"href="{job_url}#step-0-line-1""#)), "the error links to its line");
    assert!(page.contains(&format!(r#"href="{job_url}#step-0-line-2""#)), "so does the warning");
    assert!(page.contains("mismatched &#60;types&#62;") && !page.contains("<types>"), "messages are escaped");
    assert!(page.contains("E0308") && page.contains("src/a.rs:3"));
    // The second step failed without an error of its own.
    assert!(page.contains(&format!(r#"href="{job_url}#step-1""#)) && page.contains("failed."));
    assert!(page.contains("And 1 more in the jobs"), "51 annotations, 50 listed");
}
