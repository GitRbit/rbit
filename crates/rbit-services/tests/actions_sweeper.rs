//! Actions stream S: the sweep — zombie and endless tasks fail with their reason in the
//! log (and take an ephemeral runner with them), abandoned jobs are cancelled, logs expire
//! by age and by the instance budget, old runs and stale registration tokens go.

use rbit_core::{
    PushSource, Timestamp,
    actions::{LogState, RunnerCapabilities, Status},
};
use rbit_services::actions::logs::LogRange;
use rbit_storage::sqlite::{actions_runners::Declaration, actions_runs::Claimed};

use crate::actions_fixture::{Fixture, workflow};

const DAY: i64 = 86_400;

/// A push run of `alice/rocket` with one job, claimed by a runner (`seed` keeps runners
/// apart).
async fn claimed(fixture: &Fixture, seed: u8) -> Claimed {
    fixture.commit(&[(".forgejo/workflows/ci.yml", &workflow("CI", "push"))], &format!("CI {seed}"));
    let update = fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    fixture.actions().handle_push(repo, PushSource::User(fixture.alice.id()), &[update]).await.unwrap();
    fixture.claim(&["ubuntu-latest"], seed).await.unwrap()
}

#[tokio::test]
async fn silent_tasks_fail_with_a_reason_and_take_ephemeral_runners_along() {
    let fixture = Fixture::new().await;
    let actions = fixture.actions();
    let claimed = claimed(&fixture, 1).await;
    let runner = claimed.task.runner_id.unwrap();
    let runners = fixture.database.actions_runners();
    let declaration = Declaration {
        name: "once",
        labels: &["ubuntu-latest".parse().unwrap()],
        version: "0.2.0",
        ephemeral: true,
        capabilities: &RunnerCapabilities::default(),
        protocol: 1,
        now: Timestamp::now(),
    };
    runners.declare(runner, &declaration).await.unwrap();

    let now = Timestamp::now();
    assert_eq!(actions.sweep(now).await.unwrap().zombies, 0, "a fresh heartbeat");
    let later = Timestamp(now.0 + 601);
    assert_eq!(actions.sweep(later).await.unwrap().zombies, 1);
    let store = fixture.database.actions_runs();
    let task = store.task(claimed.task.id).await.unwrap().unwrap();
    assert_eq!((task.status, task.log_state), (Status::Failure, LogState::Final));
    assert_eq!(store.job(claimed.job.id).await.unwrap().unwrap().status, Status::Failure);
    let repo = fixture.repo(&fixture.alice).await;
    let log = actions.task_log(&repo, task.id, LogRange::From(0), 10, 4096).await.unwrap();
    assert_eq!(log.lines, ["rbit: the runner sent no word for 600 seconds, so the task failed"]);
    assert_eq!(runners.find(runner).await.unwrap(), None, "the ephemeral runner went with its task");
    assert_eq!(actions.sweep(later).await.unwrap().zombies, 0, "once");
}

#[tokio::test]
async fn endless_tasks_fail_and_abandoned_jobs_are_cancelled() {
    let fixture = Fixture::with(|config| {
        config.actions.zombie_timeout_seconds = 100 * DAY.unsigned_abs();
        config.actions.endless_timeout_seconds = 100;
        config.actions.abandoned_timeout_seconds = 1_000;
    })
    .await;
    let actions = fixture.actions();
    let claimed = claimed(&fixture, 2).await;
    let now = Timestamp::now();
    let swept = actions.sweep(Timestamp(now.0 + 101)).await.unwrap();
    assert_eq!((swept.zombies, swept.endless), (0, 1));
    let task = fixture.database.actions_runs().task(claimed.task.id).await.unwrap().unwrap();
    assert_eq!(task.status, Status::Failure);

    // A job nobody takes.
    fixture.commit(&[(".forgejo/workflows/ci.yml", &workflow("CI", "push").replace("ubuntu-latest", "nobody-has-this"))], "Exotic");
    let update = fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let run = actions.handle_push(repo, PushSource::User(fixture.alice.id()), &[update]).await.unwrap().remove(0);
    assert_eq!(actions.sweep(Timestamp(now.0 + 500)).await.unwrap().abandoned, 0);
    let swept = actions.sweep(Timestamp(now.0 + 2_000)).await.unwrap();
    let jobs = fixture.database.actions_runs().jobs(run.id).await.unwrap();
    assert_eq!(swept.abandoned, 1, "{jobs:?}");
    let run = fixture.database.actions_runs().run(run.id).await.unwrap().unwrap();
    assert_eq!(run.status, Status::Cancelled);
}

#[tokio::test]
async fn logs_expire_by_age_and_budget_and_old_runs_go() {
    let fixture = Fixture::new().await;
    let actions = fixture.actions();
    let first = claimed(&fixture, 3).await;
    let now = Timestamp::now();
    actions.sweep(Timestamp(now.0 + 601)).await.unwrap();
    let second = claimed(&fixture, 4).await;
    actions.sweep(Timestamp(now.0 + 1_300)).await.unwrap();
    let store = fixture.database.actions_runs();
    let logs = fixture.dir.path().join("data").join("actions").join("logs");
    assert!(logs.join(first.job.repo_id.0.to_string()).join(format!("{}.log.gz", first.task.id.0)).exists());

    // By age: both finished runs lose their logs after the retention period.
    let swept = actions.sweep(Timestamp(now.0 + 91 * DAY)).await.unwrap();
    assert_eq!(swept.expired_logs, 2);
    assert!(swept.deleted_tokens >= 2, "the registration tokens of the test runners");
    for task in [first.task.id, second.task.id] {
        assert_eq!(store.task(task).await.unwrap().unwrap().log_state, LogState::Expired);
    }
    assert!(!logs.join(first.job.repo_id.0.to_string()).join(format!("{}.log.gz", first.task.id.0)).exists(), "files deleted");
    let run = store.run(first.job.run_id).await.unwrap().unwrap();
    assert!(run.logs_expired_at.is_some(), "the run stays, marked");
    let swept = actions.sweep(Timestamp(now.0 + 366 * DAY)).await.unwrap();
    assert_eq!(swept.deleted_runs, 2);
    assert_eq!(store.run(first.job.run_id).await.unwrap(), None);
}

#[tokio::test]
async fn files_whose_rows_are_gone_are_removed_once_old() {
    let fixture = Fixture::new().await;
    let actions = fixture.actions();
    let claimed = claimed(&fixture, 5).await;
    let actions_dir = fixture.dir.path().join("data").join("actions");
    let hours_ago = |hours: u64| std::time::SystemTime::now() - std::time::Duration::from_secs(hours * 3600);
    let file = |path: std::path::PathBuf, modified: std::time::SystemTime| {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::File::create(&path).unwrap().set_modified(modified).unwrap();
        path
    };
    let logs = actions_dir.join("logs").join(claimed.job.repo_id.0.to_string());
    let kept_log = file(logs.join(format!("{}.log", claimed.task.id.0)), hours_ago(2));
    let orphan_log = file(logs.join("999999.log.gz"), hours_ago(2));
    let deleted_repo_log = file(actions_dir.join("logs").join("424242").join("999998.idx"), hours_ago(2));
    let young_log = file(logs.join("999997.log"), hours_ago(0));
    let artifacts = actions_dir.join("artifacts");
    let kept_artifact = file(artifacts.join(claimed.job.run_id.0.to_string()).join("1.tar.gz"), hours_ago(2));
    let orphan_artifact = file(artifacts.join("999999").join("7.tar.gz"), hours_ago(2));
    let young_artifact = file(artifacts.join("999998").join("8.tar.gz"), hours_ago(0));
    let stale_upload = file(artifacts.join("tmp").join("1-abc.part"), hours_ago(25));
    let fresh_upload = file(artifacts.join("tmp").join("2-def.part"), hours_ago(2));

    assert_eq!(actions.remove_orphaned_files().await.unwrap(), 4);
    for gone in [&orphan_log, &deleted_repo_log, &orphan_artifact, &stale_upload] {
        assert!(!gone.exists(), "{}", gone.display());
    }
    assert!(!artifacts.join("999999").exists(), "the emptied run directory goes too");
    for kept in [&kept_log, &young_log, &kept_artifact, &young_artifact, &fresh_upload] {
        assert!(kept.exists(), "{}", kept.display());
    }
    let hourly = Timestamp((Timestamp::now().0 / 3600 + 1) * 3600);
    assert_eq!(actions.sweep(hourly).await.unwrap().orphaned_files, 0, "the hourly pass finds nothing left");
}

#[tokio::test]
async fn the_log_budget_prunes_the_oldest_runs_first() {
    let fixture = Fixture::with(|config| config.actions.max_log_storage_bytes = 70).await;
    let actions = fixture.actions();
    let first = claimed(&fixture, 5).await;
    let now = Timestamp::now();
    actions.sweep(Timestamp(now.0 + 601)).await.unwrap();
    let second = claimed(&fixture, 6).await;
    actions.sweep(Timestamp(now.0 + 1_300)).await.unwrap();
    let store = fixture.database.actions_runs();
    // Each log is one 67-byte reason line: the two exceed the 70-byte budget, one fits.
    assert_eq!(store.task(first.task.id).await.unwrap().unwrap().log_state, LogState::Expired, "the oldest went");
    assert_eq!(store.task(second.task.id).await.unwrap().unwrap().log_state, LogState::Final, "the newest stays");
}
