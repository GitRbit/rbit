//! Actions join (J): the server decides a job's own `if:` once its needs are done, follows
//! GitHub's default `success()` for jobs without one, and applies `strategy.fail-fast`.

use rbit_core::{
    PushSource, Timestamp,
    actions::{Run, Status},
};

use crate::actions_fixture::Fixture;

async fn pushed(fixture: &Fixture, workflow: &str, reference: &str) -> Run {
    fixture.commit(&[(".rbit/workflows/ci.yml", workflow)], "CI");
    let update = fixture.push_ref(reference).await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    fixture.actions().handle_push(repo, PushSource::User(fixture.alice.id()), &[update]).await.unwrap().remove(0)
}

async fn statuses(fixture: &Fixture, run: &Run) -> Vec<(String, Status)> {
    fixture.database.actions_runs().jobs(run.id).await.unwrap().into_iter().map(|job| (job.key, job.status)).collect()
}

fn keyed(pairs: &[(&str, Status)]) -> Vec<(String, Status)> {
    pairs.iter().map(|(key, status)| ((*key).to_owned(), *status)).collect()
}

/// A runner takes the next job and finishes it with `result`; returns the job's key.
async fn run_next(fixture: &Fixture, seed: u8, result: Status, outputs: &[(&str, &str)]) -> String {
    let claimed = fixture.claim(&["x"], seed).await.expect("a job to claim");
    let store = fixture.database.actions_runs();
    store.add_outputs(claimed.job.id, outputs, 1024).await.unwrap();
    let settled = store.finish_task(claimed.task.id, result, Timestamp::now()).await.unwrap().unwrap();
    fixture.actions().settled(&settled).await.unwrap();
    claimed.job.key
}

const CHAIN: &str = "on: push
jobs:
  build:
    runs-on: x
    steps: [{run: x}]
  deploy:
    needs: build
    runs-on: x
    steps: [{run: x}]
  cleanup:
    needs: build
    if: always()
    runs-on: x
    steps: [{run: x}]
  on-failure:
    needs: build
    if: failure()
    runs-on: x
    steps: [{run: x}]
  on-output:
    needs: build
    if: failure() && needs.build.outputs.ship == 'yes' && needs.build.result == 'failure'
    runs-on: x
    steps: [{run: x}]
";

#[tokio::test]
async fn a_failed_need_skips_by_default_and_conditions_decide_the_rest() {
    let fixture = Fixture::new().await;
    let run = pushed(&fixture, CHAIN, "refs/heads/main").await;
    assert_eq!(run_next(&fixture, 1, Status::Failure, &[("ship", "yes")]).await, "build");
    assert_eq!(
        statuses(&fixture, &run).await,
        keyed(&[
            ("build", Status::Failure),
            ("deploy", Status::Skipped),
            ("cleanup", Status::Waiting),
            ("on-failure", Status::Waiting),
            ("on-output", Status::Waiting),
        ])
    );
}

#[tokio::test]
async fn success_skips_the_failure_branch() {
    let fixture = Fixture::new().await;
    let run = pushed(&fixture, CHAIN, "refs/heads/main").await;
    run_next(&fixture, 1, Status::Success, &[]).await;
    assert_eq!(
        statuses(&fixture, &run).await,
        keyed(&[
            ("build", Status::Success),
            ("deploy", Status::Waiting),
            ("cleanup", Status::Waiting),
            ("on-failure", Status::Skipped),
            ("on-output", Status::Skipped),
        ])
    );
}

#[tokio::test]
async fn conditions_without_needs_are_decided_when_the_run_starts() {
    let fixture = Fixture::new().await;
    let workflow = "on: push\njobs:\n  release:\n    if: github.ref == 'refs/heads/main' && github.actor == 'alice'\n    runs-on: x\n    steps: [{run: x}]\n  broken:\n    if: fromJSON('not json')\n    runs-on: x\n    steps: [{run: x}]\n";
    let main = pushed(&fixture, workflow, "refs/heads/main").await;
    assert_eq!(statuses(&fixture, &main).await, keyed(&[("release", Status::Waiting), ("broken", Status::Failure)]));
    let other = pushed(&fixture, workflow, "refs/heads/topic").await;
    assert_eq!(statuses(&fixture, &other).await, keyed(&[("release", Status::Skipped), ("broken", Status::Failure)]));
    assert_eq!(fixture.database.actions_runs().run(other.id).await.unwrap().unwrap().status, Status::Failure);
    assert!(fixture.database.actions_runs().undecided(10).await.unwrap().is_empty(), "nothing waits for a decision");
}

#[tokio::test]
async fn fail_fast_cancels_matrix_siblings_unless_turned_off() {
    let fixture = Fixture::new().await;
    let workflow = |fail_fast: bool| {
        format!(
            "on: push\njobs:\n  test:\n    strategy:\n      fail-fast: {fail_fast}\n      matrix: {{n: [1, 2, 3]}}\n    runs-on: x\n    steps: [{{run: x}}]\n"
        )
    };
    let run = pushed(&fixture, &workflow(true), "refs/heads/main").await;
    run_next(&fixture, 1, Status::Failure, &[]).await;
    let after: Vec<Status> = statuses(&fixture, &run).await.into_iter().map(|(_, status)| status).collect();
    assert_eq!(after, [Status::Failure, Status::Cancelled, Status::Cancelled]);
    // A cancelled job outranks a failed one in a run's status (`rbit_core::actions::aggregate`).
    assert_eq!(fixture.database.actions_runs().run(run.id).await.unwrap().unwrap().status, Status::Cancelled);

    let run = pushed(&fixture, &workflow(false), "refs/heads/next").await;
    run_next(&fixture, 2, Status::Failure, &[]).await;
    let after: Vec<Status> = statuses(&fixture, &run).await.into_iter().map(|(_, status)| status).collect();
    assert_eq!(after, [Status::Failure, Status::Waiting, Status::Waiting]);
}

#[tokio::test]
async fn max_parallel_lets_one_matrix_job_run_at_a_time() {
    let fixture = Fixture::new().await;
    let workflow = "on: push\njobs:\n  test:\n    strategy:\n      max-parallel: 1\n      matrix: {n: [1, 2]}\n    runs-on: x\n    steps: [{run: x}]\n";
    pushed(&fixture, workflow, "refs/heads/main").await;
    let first = fixture.claim(&["x"], 1).await.expect("one job runs");
    assert!(fixture.claim(&["x"], 2).await.is_none(), "max-parallel: 1");
    let store = fixture.database.actions_runs();
    let settled = store.finish_task(first.task.id, Status::Success, Timestamp::now()).await.unwrap().unwrap();
    fixture.actions().settled(&settled).await.unwrap();
    assert!(fixture.claim(&["x"], 3).await.is_some(), "the next one runs once the first finished");
}
