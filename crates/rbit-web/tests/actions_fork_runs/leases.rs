//! A task's lease: only the runner a task was given to, and only while the server still
//! counts the task as the job's current attempt, may report on it, append to its log,
//! upload its artifacts and caches, or use its token. A task the sweeper failed, the task of
//! an attempt a re-run replaced, a cancelled task and another runner's credential complete
//! nothing; a retried poll revives none of them.

use axum::http::{Method, StatusCode};
use rbit_core::{
    Timestamp,
    actions::{Status, TaskId},
};
use rbit_storage::sqlite::actions_runs::Rerun;
use serde_json::json;

use crate::{
    actions_artifacts::{call, hex, upload},
    actions_runner::Fixture,
    actions_runner_tasks::{payload, poll, seed, spec, state, template},
};

async fn quick() -> Fixture {
    Fixture::with(|config| config.actions.long_poll_seconds = 1).await
}

fn step(index: u32, status: &str) -> serde_json::Value {
    json!({"index": index, "status": status, "started_at": 100, "stopped_at": null, "log_index": 0, "log_length": 0})
}

fn heartbeat() -> serde_json::Value {
    json!({"steps": [], "outputs": {}, "result": null})
}

fn success() -> serde_json::Value {
    json!({"steps": [step(0, "success"), step(1, "success")], "outputs": {"version": "stale"}, "result": "success"})
}

async fn append(fixture: &Fixture, credential: &str, task: i64, from: u64, text: &str) -> StatusCode {
    fixture.call(&format!("tasks/{task}/logs?from={from}"), Some(credential), text.to_owned()).await.status
}

async fn save_cache(fixture: &Fixture, credential: &str, task: i64, key: &str) -> StatusCode {
    let path = format!("tasks/{task}/caches?key={key}&version={}", hex(key.as_bytes()));
    call(fixture, Method::PUT, &path, credential, Some(&hex(b"cache")), b"cache".to_vec()).await.status
}

/// Whether `token` still authenticates as a task.
async fn token_lives(fixture: &Fixture, token: &str) -> bool {
    fixture.actions().authenticate_task(token).await.unwrap().is_some()
}

#[tokio::test]
async fn a_task_the_sweeper_failed_is_told_to_stop_and_completes_nothing() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let claimed = spec(&poll(&fixture, &credential, Some("k")).await.1);
    let (task, token) = (claimed.task_id.0, claimed.token.expose().to_owned());
    assert_eq!(state(&fixture, &credential, task, &heartbeat()).await, (StatusCode::OK, json!({"cancel": false})));

    // The runner falls silent for longer than the zombie timeout: its lease is gone.
    let swept = fixture.actions().sweep(Timestamp(Timestamp::now().0 + 601)).await.unwrap();
    assert_eq!(swept.zombies, 1);
    assert!(!token_lives(&fixture, &token).await, "the token died with the lease");

    // The runner comes back still running the job: it must be told to stop.
    assert_eq!(
        state(&fixture, &credential, task, &heartbeat()).await,
        (StatusCode::OK, json!({"cancel": true})),
        "a task the server stopped keeps no runner busy"
    );
    assert_eq!(append(&fixture, &credential, task, 0, "late\n").await, StatusCode::CONFLICT);
    assert_eq!(upload(&fixture, &credential, task, "late", "", b"late").await.status, StatusCode::CONFLICT);
    assert_eq!(save_cache(&fixture, &credential, task, "late").await, StatusCode::CONFLICT);
    assert_eq!(state(&fixture, &credential, task, &success()).await.0, StatusCode::OK);
    let store = fixture.database.actions_runs();
    let job = &store.jobs(run.id).await.unwrap()[0];
    assert_eq!((job.status, store.task(TaskId(task)).await.unwrap().unwrap().status), (Status::Failure, Status::Failure));
    assert_eq!(store.outputs(job.id).await.unwrap(), Vec::new(), "a stale result leaves no outputs");
    assert_eq!(store.run(run.id).await.unwrap().unwrap().status, Status::Failure);

    // Retrying the claiming poll hands out nothing: no fresh token for a dead task.
    assert_eq!(poll(&fixture, &credential, Some("k")).await.0, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn another_runners_credential_reaches_none_of_a_tasks_calls() {
    let fixture = quick().await;
    let owner = fixture.runner(&["linux"]).await;
    let other = fixture.runner(&["linux"]).await;
    seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let task = spec(&poll(&fixture, &owner, Some("o")).await.1).task_id.0;

    assert_eq!(state(&fixture, &other, task, &success()).await.0, StatusCode::FORBIDDEN);
    assert_eq!(append(&fixture, &other, task, 0, "forged\n").await, StatusCode::FORBIDDEN);
    assert_eq!(upload(&fixture, &other, task, "forged", "", b"forged").await.status, StatusCode::FORBIDDEN);
    assert_eq!(save_cache(&fixture, &other, task, "forged").await, StatusCode::FORBIDDEN);
    let listed = call(&fixture, Method::GET, &format!("tasks/{task}/artifacts"), &other, None, Vec::new()).await;
    assert_eq!(listed.status, StatusCode::FORBIDDEN);
    let lookup = serde_json::to_vec(&json!({"key": "forged", "restore_keys": [], "version": hex(b"forged")})).unwrap();
    let found = call(&fixture, Method::POST, &format!("tasks/{task}/caches/lookup"), &other, None, lookup).await;
    assert_eq!(found.status, StatusCode::FORBIDDEN);

    let store = fixture.database.actions_runs();
    let stored = store.task(TaskId(task)).await.unwrap().unwrap();
    assert_eq!((stored.status, stored.log_lines), (Status::Running, 0), "nothing changed");
    assert_eq!(state(&fixture, &owner, task, &heartbeat()).await, (StatusCode::OK, json!({"cancel": false})));
}

#[tokio::test]
async fn a_replaced_attempt_keeps_neither_its_token_nor_its_lease() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let old = spec(&poll(&fixture, &credential, Some("old")).await.1);
    let repo = fixture.repo().await;
    fixture.actions().cancel(&fixture.alice, &repo, run.id).await.unwrap();
    fixture.actions().rerun(&fixture.alice, &repo, run.id, Rerun::All).await.unwrap();
    let new = spec(&poll(&fixture, &credential, Some("new")).await.1);
    let (old_task, new_task) = (old.task_id.0, new.task_id.0);

    assert!(!token_lives(&fixture, old.token.expose()).await, "the previous attempt's token is dead");
    assert!(token_lives(&fixture, new.token.expose()).await);
    // Retrying the old claim mints nothing, and leaves the new attempt's token alone.
    assert_eq!(poll(&fixture, &credential, Some("old")).await.0, StatusCode::NO_CONTENT);
    assert!(token_lives(&fixture, new.token.expose()).await);
    assert_eq!(save_cache(&fixture, &credential, old_task, "old").await, StatusCode::CONFLICT);
    assert_eq!(upload(&fixture, &credential, old_task, "old", "", b"old").await.status, StatusCode::CONFLICT);
    assert_eq!(state(&fixture, &credential, old_task, &success()).await, (StatusCode::OK, json!({"cancel": true})));

    let store = fixture.database.actions_runs();
    let job = &store.jobs(run.id).await.unwrap()[0];
    assert_eq!((job.status, job.task_id, job.attempt), (Status::Running, Some(TaskId(new_task)), 2));
    assert_eq!(store.outputs(job.id).await.unwrap(), Vec::new());
    assert_eq!(save_cache(&fixture, &credential, new_task, "new").await, StatusCode::CREATED);
    assert_eq!(state(&fixture, &credential, new_task, &success()).await, (StatusCode::OK, json!({"cancel": false})));
    assert_eq!(store.jobs(run.id).await.unwrap()[0].status, Status::Success);
}

#[tokio::test]
async fn a_cancelled_task_cannot_revive_its_token_or_succeed() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let claimed = spec(&poll(&fixture, &credential, Some("k")).await.1);
    let task = claimed.task_id.0;
    let repo = fixture.repo().await;
    fixture.actions().cancel(&fixture.alice, &repo, run.id).await.unwrap();

    assert!(!token_lives(&fixture, claimed.token.expose()).await, "dead at the cancel");
    assert_eq!(poll(&fixture, &credential, Some("k")).await.0, StatusCode::NO_CONTENT, "a retried poll revives nothing");
    assert_eq!(state(&fixture, &credential, task, &heartbeat()).await, (StatusCode::OK, json!({"cancel": true})));
    // Its cancellation steps may still log; its result changes nothing.
    assert_eq!(append(&fixture, &credential, task, 0, "cleaning up\n").await, StatusCode::OK);
    assert_eq!(state(&fixture, &credential, task, &success()).await, (StatusCode::OK, json!({"cancel": true})));
    let store = fixture.database.actions_runs();
    assert_eq!(store.jobs(run.id).await.unwrap()[0].status, Status::Cancelled);
    assert_eq!(store.run(run.id).await.unwrap().unwrap().status, Status::Cancelled);
    assert!(!token_lives(&fixture, claimed.token.expose()).await);
}
