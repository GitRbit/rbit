//! Real `SQLite` work counts and authorization for the used batched job-page read.
use crate::actions_fixture::{Fixture, workflow};
use rbit_core::{
    PushSource, RepoAccess, Timestamp, Visibility,
    actions::{LogState, Status},
};
use rbit_services::{
    CreateRepository, ServiceError,
    actions::logs::{LogLimits, LogRange},
    repo_settings::UpdateRepository,
};
use rbit_storage::{
    count_work,
    sqlite::actions_runs::{Claimed, Rerun, StepUpdate},
};

const LIMITS: LogLimits = LogLimits { lines: 1000, bytes: 256 * 1024 };

async fn seed(fixture: &Fixture, steps: usize) -> Claimed {
    fixture.commit(&[(".forgejo/workflows/ci.yml", &workflow("CI", "push"))], "Log fixture");
    let update = fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.alice).await;
    fixture.actions().handle_push(repo.repo.repo.id, PushSource::User(fixture.alice.id()), &[update]).await.unwrap();
    let claimed = fixture.claim(&["ubuntu-latest"], 1).await.unwrap();
    let store = fixture.database.actions_runs();
    let names: Vec<_> = (0..steps).map(|index| format!("step {index}")).collect();
    store.create_steps(claimed.task.id, &names).await.unwrap();
    let updates: Vec<_> = (0..steps)
        .map(|index| StepUpdate {
            number: i64::try_from(index).unwrap(),
            name: "",
            status: Status::Success,
            started_at: Some(Timestamp(1)),
            stopped_at: Some(Timestamp(2)),
            log_index: i64::try_from(index).unwrap(),
            log_length: 1,
        })
        .collect();
    store.update_steps(claimed.task.id, &updates).await.unwrap();
    let directory = fixture.dir.path().join("data/actions/logs").join(repo.repo.repo.id.0.to_string());
    std::fs::create_dir_all(&directory).unwrap();
    let mut bytes = Vec::new();
    let mut ends = Vec::new();
    for index in 0..steps {
        bytes.extend_from_slice(format!("line {index} <escaped>\n").as_bytes());
        ends.extend_from_slice(&u64::try_from(bytes.len()).unwrap().to_le_bytes());
    }
    std::fs::write(directory.join(format!("{}.log", claimed.task.id.0)), &bytes).unwrap();
    std::fs::write(directory.join(format!("{}.idx", claimed.task.id.0)), ends).unwrap();
    store.set_log_progress(claimed.task.id, i64::try_from(steps).unwrap(), i64::try_from(bytes.len()).unwrap(), false).await.unwrap();
    claimed
}

#[tokio::test]
async fn one_authorized_snapshot_has_constant_sql_work_and_exact_legacy_windows() {
    for steps in [1, 8, 32, 128, 512] {
        let fixture = Fixture::new().await;
        let claimed = seed(&fixture, steps).await;
        let repo = fixture.repo(&fixture.alice).await;
        let actions = fixture.actions();
        let run = fixture.database.actions_runs().run(claimed.job.run_id).await.unwrap().unwrap();
        let (result, work) = count_work(actions.job_log_detail(&repo, run.number, claimed.job.id, LIMITS)).await;
        let result = result.unwrap();
        assert_eq!(work.statements, 6, "independent of {steps} started steps");
        assert_eq!(work.acquisitions, 6);
        assert_eq!(work.cache_writes, 0);
        assert_eq!(result.detail.steps.len(), steps);
        let (legacy, old_work) = count_work(async {
            let mut windows = Vec::new();
            for step in &result.detail.steps {
                windows.push(Some(
                    actions.step_log(&repo, claimed.task.id, step.number, LogRange::Tail, LIMITS.lines, LIMITS.bytes).await.unwrap(),
                ));
            }
            windows
        })
        .await;
        assert_eq!(result.logs, legacy);
        assert_eq!(old_work.statements, 3 * u64::try_from(steps).unwrap());
        actions.finalise_log(claimed.task.id).await.unwrap();
        let final_read = actions.job_log_detail(&repo, run.number, claimed.job.id, LIMITS).await.unwrap();
        for (before, after) in result.logs.iter().zip(&final_read.logs) {
            assert_eq!(before.as_ref().unwrap().lines, after.as_ref().unwrap().lines);
            assert!(!after.as_ref().unwrap().expired);
        }
    }
}

#[tokio::test]
async fn hidden_disabled_wrong_run_and_cross_repository_jobs_are_denied_before_log_io() {
    let fixture = Fixture::new().await;
    let claimed = seed(&fixture, 2).await;
    let actions = fixture.actions();
    let repo = fixture.repo(&fixture.alice).await;
    let run = fixture.database.actions_runs().run(claimed.job.run_id).await.unwrap().unwrap();
    // Deliberately corrupt selected offsets: denial must happen before decoding them.
    let index = fixture.dir.path().join(format!("data/actions/logs/{}/{}.idx", repo.repo.repo.id.0, claimed.task.id.0));
    std::fs::write(index, [9u64.to_le_bytes(), 8u64.to_le_bytes()].concat()).unwrap();
    let mut hidden = repo.clone();
    hidden.access = RepoAccess::None;
    let (denied, work) = count_work(actions.job_log_detail(&hidden, run.number, claimed.job.id, LIMITS)).await;
    assert!(matches!(denied, Err(ServiceError::NotFound)));
    assert_eq!(work.statements, 0);
    assert!(matches!(actions.job_log_detail(&repo, run.number + 1, claimed.job.id, LIMITS).await, Err(ServiceError::NotFound)));
    fixture
        .services
        .repositories
        .create(&fixture.alice, CreateRepository { name: "other".into(), description: String::new(), visibility: Visibility::Public })
        .await
        .unwrap();
    let other = fixture.services.repositories.open(Some(&fixture.alice), "alice", "other").await.unwrap();
    actions.set_repo_enabled(&fixture.alice, &other, true).await.unwrap();
    assert!(matches!(actions.job_log_detail(&other, run.number, claimed.job.id, LIMITS).await, Err(ServiceError::NotFound)));
    actions.set_repo_enabled(&fixture.alice, &repo, false).await.unwrap();
    assert!(matches!(actions.job_log_detail(&repo, run.number, claimed.job.id, LIMITS).await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn private_repository_revocation_is_resolved_before_the_next_read() {
    let fixture = Fixture::new().await;
    let claimed = seed(&fixture, 1).await;
    let repo = fixture.repo(&fixture.alice).await;
    fixture.services.repo_settings.update(&repo, UpdateRepository { visibility: Visibility::Private, ..Default::default() }).await.unwrap();
    let run = fixture.database.actions_runs().run(claimed.job.run_id).await.unwrap().unwrap();
    let reader = fixture.repo(&fixture.rita).await;
    assert!(fixture.actions().job_log_detail(&reader, run.number, claimed.job.id, LIMITS).await.is_ok());
    fixture.services.collaborators.remove(&fixture.alice, &repo, "rita").await.unwrap();
    assert!(matches!(fixture.services.repositories.open(Some(&fixture.rita), "alice", "rocket").await, Err(ServiceError::NotFound)));
    assert!(matches!(fixture.services.repositories.open(Some(&fixture.bob), "alice", "rocket").await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn recorded_expiry_and_not_started_preserve_distinct_window_states() {
    let fixture = Fixture::new().await;
    let claimed = seed(&fixture, 2).await;
    let repo = fixture.repo(&fixture.alice).await;
    let store = fixture.database.actions_runs();
    let run = store.run(claimed.job.run_id).await.unwrap().unwrap();
    store
        .update_steps(
            claimed.task.id,
            &[StepUpdate { number: 1, name: "", status: Status::Waiting, started_at: None, stopped_at: None, log_index: 1, log_length: 0 }],
        )
        .await
        .unwrap();
    fixture.actions().delete_log(repo.repo.repo.id, claimed.task.id).await.unwrap();
    store.set_log_state(claimed.task.id, LogState::Expired).await.unwrap();
    let read = fixture.actions().job_log_detail(&repo, run.number, claimed.job.id, LIMITS).await.unwrap();
    assert!(read.logs[0].as_ref().unwrap().expired);
    assert!(!read.logs[0].as_ref().unwrap().truncated);
    assert!(read.logs[1].is_none());
}

#[tokio::test]
async fn a_new_attempt_reads_only_the_jobs_current_task_even_when_old_files_remain() {
    let fixture = Fixture::new().await;
    let claimed = seed(&fixture, 1).await;
    let actions = fixture.actions();
    let repo = fixture.repo(&fixture.alice).await;
    let store = fixture.database.actions_runs();
    let run = store.run(claimed.job.run_id).await.unwrap().unwrap();
    let old = actions.job_log_detail(&repo, run.number, claimed.job.id, LIMITS).await.unwrap();
    assert_eq!(old.logs[0].as_ref().unwrap().lines, ["line 0 <escaped>"]);
    store.finish_task(claimed.task.id, Status::Success, Timestamp::now()).await.unwrap().unwrap();
    actions.rerun(&fixture.alice, &repo, run.id, Rerun::All).await.unwrap();
    let waiting = actions.job_log_detail(&repo, run.number, claimed.job.id, LIMITS).await.unwrap();
    assert!(waiting.detail.job.task_id.is_none() && waiting.logs.is_empty());
    let next = fixture.claim(&["ubuntu-latest"], 2).await.unwrap();
    assert_ne!(next.task.id, claimed.task.id);
    store.create_steps(next.task.id, &["new attempt".into()]).await.unwrap();
    store
        .update_steps(
            next.task.id,
            &[StepUpdate {
                number: 0,
                name: "",
                status: Status::Running,
                started_at: Some(Timestamp::now()),
                stopped_at: None,
                log_index: 0,
                log_length: 0,
            }],
        )
        .await
        .unwrap();
    let current = actions.job_log_detail(&repo, run.number, claimed.job.id, LIMITS).await.unwrap();
    assert_eq!(current.detail.job.task_id, Some(next.task.id));
    assert_eq!(current.logs[0].as_ref().unwrap().lines, Vec::<String>::new());
    assert!(fixture.dir.path().join(format!("data/actions/logs/{}/{}.log", repo.repo.repo.id.0, claimed.task.id.0)).exists());
}
