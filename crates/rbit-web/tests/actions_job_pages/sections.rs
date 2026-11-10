//! "Set up job" and "Complete job": the runner's lines before the first step and after
//! the last, shown and tailed like steps; a skipped step's reason; line times.
use super::*;

#[tokio::test]
async fn set_up_and_complete_job_frame_the_steps() {
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let mut build = template("build", &["linux"], &[]);
    build.steps.push(build.steps[0].clone());
    let run = seed(&fixture, &[payload(&build)]).await;
    let job = fixture.database.actions_runs().jobs(run.id).await.unwrap().remove(0);
    let job_url = format!("/alice/rocket/actions/runs/{}/jobs/{}", run.number, job.id.0);
    let task = spec(&poll(&fixture, &credential, Some("sections")).await.1).task_id.0;

    // Before any step: everything is "Set up job", and the tail follows it.
    logs(&fixture, &credential, task, 0, "2026-11-10T12:00:01.250Z Current runner version: '0.2.0'\nRunner name: 'box'\n").await;
    let (_, _, page) = get(&fixture, &job_url, false).await;
    assert!(page.contains(r#"<details open data-step="setup" data-state="running">"#), "{page}");
    assert!(page.contains("Set up job") && page.contains(r#"id="step-setup-line-2""#));
    assert!(page.contains(r#"<time class="log-time" datetime="2026-11-10T12:00:01.250Z">12:00:01</time>"#));
    assert!(page.contains(r#"data-log="setup" data-first="1" data-next="2""#), "the set-up lines are tailed");

    logs(
        &fixture,
        &credential,
        task,
        2,
        "Complete job name: build\nstep one\nThis step was skipped because an earlier step failed\nJob completed with result: failure\n",
    )
    .await;
    state(
        &fixture,
        &credential,
        task,
        &json!({"steps": [
            {"index": 0, "status": "failure", "started_at": 1, "stopped_at": 2, "log_index": 3, "log_length": 1},
            {"index": 1, "status": "skipped", "started_at": null, "stopped_at": null, "log_index": 4, "log_length": 1},
        ], "outputs": {}, "result": "failure"}),
    )
    .await;
    let (_, _, page) = get(&fixture, &job_url, false).await;
    let setup = page.find(r#"data-step="setup""#).unwrap();
    let first = page.find(r#"data-step="0""#).unwrap();
    let skipped = page.find(r#"data-step="1""#).unwrap();
    let complete = page.find(r#"data-step="complete""#).unwrap_or_else(|| panic!("{page}"));
    assert!(setup < first && first < skipped && skipped < complete, "the sections frame the steps");
    assert!(page.contains(r#"<details data-step="setup" data-state="success">"#));
    assert!(page.contains("Complete job name: build") && page.contains(r#"id="step-0-line-1""#));
    assert!(page.contains(r#"id="step-1-line-1""#) && page.contains("This step was skipped because an earlier step failed"));
    assert!(page.contains(r#"data-step="complete" data-state="failure""#) && page.contains(r#"id="step-complete-line-1""#));
    assert!(page.contains("Job completed with result: failure"));
    assert!(!page.contains("data-next="), "a finished job has no tail");
    assert!(page.contains("3 steps") && !page.contains("5 steps"), "the sections are not counted as steps");
}
