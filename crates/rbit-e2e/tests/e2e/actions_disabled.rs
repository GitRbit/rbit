//! With `[actions] enabled = false` (the default), Actions leaves no trace: a repository
//! full of workflow files is pushed, forked and proposed from the fork, a dispatch is
//! attempted through the API, and afterwards every `actions_*` table is empty and
//! `<data_dir>/actions` does not exist.
//!
//! The Actions foundation (workstream F) owns this file.

use std::time::{Duration, Instant};

use rbit_e2e::{Git, PASSWORD, StatusCode, TestServer};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

/// A workflow that would run on every event this test causes.
const WORKFLOW: &str = "\
name: CI
on:
  push:
  pull_request:
  workflow_dispatch:
  schedule:
    - cron: '*/5 * * * *'
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
";

/// Rows in every `actions_*` table, found through `sqlite_master` so tables added later are
/// covered too.
async fn actions_rows(server: &TestServer) -> Vec<(String, i64)> {
    let options = SqliteConnectOptions::new().filename(server.data_dir().join("rbit.db")).read_only(true);
    let pool: SqlitePool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.expect("open the database");
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' AND name GLOB 'actions_*' ORDER BY name")
            .fetch_all(&pool)
            .await
            .expect("list the Actions tables");
    let mut counts = Vec::new();
    for table in tables {
        // Table names come from sqlite_master and match `actions_*`.
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM \"{table}\"")))
            .fetch_one(&pool)
            .await
            .expect("count rows");
        counts.push((table, count));
    }
    pool.close().await;
    counts
}

#[tokio::test]
async fn workflows_pushes_pull_requests_and_dispatches_leave_no_trace() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    let sample = git.sample();
    git.commit_file(&sample.dir, ".forgejo/workflows/ci.yml", WORKFLOW);
    git.commit_file(&sample.dir, ".github/workflows/ci.yml", WORKFLOW);
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", PASSWORD));

    // A pull request from a fork.
    let bob = server.sign_up("bob").await;
    bob.post("/alice/demo/fork", &[("owner", "bob"), ("name", "")]).await.expect_redirect("/bob/demo");
    let work = git.clone(&server.git_url_as("bob", "demo", "bob", PASSWORD));
    git.ok(&work, &["checkout", "--quiet", "-b", "change"]);
    git.commit_file(&work, ".forgejo/workflows/extra.yml", WORKFLOW);
    git.ok(&work, &["push", "--quiet", "origin", "change"]);
    bob.post("/alice/demo/pulls", &[("base", "main"), ("head", "bob:change"), ("title", "Run my workflow")])
        .await
        .expect_redirect("/alice/demo/pull/1");

    // A dispatch through the API is not even routed.
    let token = alice.create_token("ci", "write").await;
    let response = reqwest::Client::new()
        .post(server.url("/api/v1/repos/alice/demo/actions/workflows/ci.yml/dispatches"))
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(r#"{"ref":"main"}"#)
        .send()
        .await
        .expect("dispatch request");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = reqwest::Client::new()
        .post(server.url("/api/actions/runner.v1.RunnerService/Register"))
        .header("content-type", "application/proto")
        .body(Vec::new())
        .send()
        .await
        .expect("register request");
    assert_eq!(response.status(), StatusCode::NOT_FOUND, "the runner endpoint does not exist");

    // The pull request's background work (mergeability) shows the event bus has delivered
    // the push and pull request events to every subscriber.
    let deadline = Instant::now() + Duration::from_secs(30);
    while !alice.get("/alice/demo/pull/1").await.contains("No conflicts with the base branch") {
        assert!(Instant::now() < deadline, "the pull request never became mergeable");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;

    let rows = actions_rows(&server).await;
    assert!(rows.len() >= 16, "the Actions tables exist: {rows:?}");
    let written: Vec<_> = rows.iter().filter(|(_, count)| *count > 0).collect();
    assert!(written.is_empty(), "nothing is written while Actions is disabled: {written:?}");
    assert!(!server.data_dir().join("actions").exists(), "no Actions directory is created");
}
