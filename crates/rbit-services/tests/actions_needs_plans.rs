//! Actions: plans from needs. A job whose matrix or `runs-on` reads an earlier job's outputs
//! waits as one blocked stand-in; once its needs are done the server plans it with their
//! outputs and puts the jobs it gives in its place, as GitHub's dynamic matrices work.

use rbit_core::{
    PushSource, Timestamp,
    actions::{Run, Status},
};

use crate::actions_fixture::Fixture;

const DYNAMIC: &str = r"on: push
jobs:
  setup:
    runs-on: x
    outputs:
      matrix: ${{ steps.m.outputs.matrix }}
      label: ${{ steps.m.outputs.label }}
    steps: [{id: m, run: x}]
  build:
    needs: setup
    runs-on: ${{ needs.setup.outputs.label }}
    strategy:
      matrix: ${{ fromJSON(needs.setup.outputs.matrix) }}
    steps: [{run: x}]
  after:
    needs: build
    runs-on: x
    steps: [{run: x}]
";

async fn pushed(fixture: &Fixture, files: &[(&str, &str)]) -> Run {
    fixture.commit(files, "CI");
    let update = fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    fixture.actions().handle_push(repo, PushSource::User(fixture.alice.id()), &[update]).await.unwrap().remove(0)
}

async fn jobs(fixture: &Fixture, run: &Run) -> Vec<(String, String, Status)> {
    fixture.database.actions_runs().jobs(run.id).await.unwrap().into_iter().map(|job| (job.key, job.name, job.status)).collect()
}

/// A runner with `labels` takes the next job and finishes it with `outputs`.
async fn run_next(fixture: &Fixture, labels: &[&str], seed: u8, outputs: &[(&str, &str)], status: Status) -> String {
    let claimed = fixture.claim(labels, seed).await.expect("a job to claim");
    let store = fixture.database.actions_runs();
    store.add_outputs(claimed.job.id, outputs, 1024).await.unwrap();
    let settled = store.finish_task(claimed.task.id, status, Timestamp::now()).await.unwrap().unwrap();
    fixture.actions().settled(&settled).await.unwrap();
    claimed.job.name
}

#[tokio::test]
async fn a_matrix_and_runs_on_from_an_earlier_jobs_outputs() {
    let fixture = Fixture::new().await;
    let run = pushed(&fixture, &[(".rbit/workflows/ci.yml", DYNAMIC)]).await;
    assert!(run.error.is_none(), "{:?}", run.error);
    let listed = jobs(&fixture, &run).await;
    assert_eq!(listed.len(), 3, "the matrix job is one stand-in until `setup` is done: {listed:?}");
    assert_eq!((listed[1].0.as_str(), listed[1].2), ("build", Status::Blocked));

    let matrix = r#"{"fruit":["apple","pear"]}"#;
    assert_eq!(run_next(&fixture, &["x"], 1, &[("matrix", matrix), ("label", "big")], Status::Success).await, "setup");
    let listed = jobs(&fixture, &run).await;
    let builds: Vec<_> = listed.iter().filter(|(key, ..)| key == "build").map(|(_, name, status)| (name.as_str(), *status)).collect();
    assert_eq!(builds, [("build (apple)", Status::Waiting), ("build (pear)", Status::Waiting)], "{listed:?}");
    assert!(fixture.claim(&["x"], 2).await.is_none(), "`runs-on` came from the output: only a `big` runner takes them");

    run_next(&fixture, &["big"], 3, &[], Status::Success).await;
    let status = |listed: &[(String, String, Status)]| listed.iter().find(|(key, ..)| key == "after").unwrap().2;
    assert_eq!(status(&jobs(&fixture, &run).await), Status::Blocked, "`after` needs every combination");
    run_next(&fixture, &["big"], 4, &[], Status::Success).await;
    assert_eq!(status(&jobs(&fixture, &run).await), Status::Waiting);
}

#[tokio::test]
async fn a_bad_matrix_fails_the_job_and_a_failed_need_skips_it() {
    let fixture = Fixture::new().await;
    let run = pushed(&fixture, &[(".rbit/workflows/ci.yml", DYNAMIC)]).await;
    run_next(&fixture, &["x"], 1, &[("matrix", "not json"), ("label", "big")], Status::Success).await;
    let listed = jobs(&fixture, &run).await;
    assert_eq!(listed.iter().find(|(key, ..)| key == "build").unwrap().2, Status::Failure, "{listed:?}");

    let fixture = Fixture::new().await;
    let run = pushed(&fixture, &[(".rbit/workflows/ci.yml", DYNAMIC)]).await;
    run_next(&fixture, &["x"], 1, &[], Status::Failure).await;
    let listed = jobs(&fixture, &run).await;
    assert_eq!(listed.iter().find(|(key, ..)| key == "build").unwrap().2, Status::Skipped, "success() is the default: {listed:?}");
}
