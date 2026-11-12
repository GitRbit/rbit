//! Scratch runs (agent plan steps 3.1 and 3.3): a commit on no branch, pushed to
//! `refs/scratch/<login>/<name>`, starts the workflows a branch would; its
//! `Rbit-CI-Jobs:` trailer keeps only the named jobs and the jobs they need; the ref is
//! never listed as a branch; and nobody pushes under another person's name.
//!
//! The integration workstream (I) owns this file.

use rbit_e2e::{Git, PASSWORD, TestServer};
use serde_json::Value;

const WORKFLOW: &str = "name: CI
on: push
jobs:
  build:
    runs-on: nobody-has-this-label
    steps:
      - run: true
  tests:
    needs: build
    runs-on: nobody-has-this-label
    steps:
      - run: true
  lint:
    runs-on: nobody-has-this-label
    steps:
      - run: true
";

#[tokio::test]
async fn a_scratch_commit_runs_the_jobs_it_asks_for_and_is_no_branch() {
    let server = TestServer::start_with(|config| config.actions.enabled = true);
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "rocket", "public").await;
    crate::enable_actions(&alice, "alice", "rocket").await;
    let token = alice.create_token("agent", "admin").await;
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let get = |path: String| {
        let request = http.get(server.url(&format!("/api/v1/repos/alice/rocket/{path}"))).bearer_auth(&token);
        async move { request.send().await.unwrap().json::<Value>().await.unwrap() }
    };

    let git = Git::new();
    let sample = git.sample();
    let url = server.git_url_as("alice", "rocket", "alice", PASSWORD);
    std::fs::create_dir_all(sample.dir.join(".rbit/workflows")).unwrap();
    git.commit_file(&sample.dir, ".rbit/workflows/ci.yml", WORKFLOW);
    git.ok(&sample.dir, &["push", "--quiet", &url, "main"]);

    // A commit on no branch, asking for `tests` only: it runs `tests` and the `build` it needs.
    std::fs::write(sample.dir.join("change.txt"), "uncommitted\n").unwrap();
    git.ok(&sample.dir, &["add", "change.txt"]);
    let tree = git.ok(&sample.dir, &["write-tree"]).trim().to_owned();
    let message = "Try a change\n\nRbit-CI-Jobs: tests\n";
    let sha = git.ok(&sample.dir, &["commit-tree", &tree, "-p", "HEAD", "-m", message]).trim().to_owned();
    git.ok(&sample.dir, &["push", "--quiet", &url, &format!("{sha}:refs/scratch/alice/try")]);
    // The push starts its run through the event subscriber, a moment later.
    let mut runs = Value::Null;
    for _ in 0..100 {
        runs = get(format!("actions/runs?head_sha={sha}")).await;
        if runs["total_count"] == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let run = &runs["workflow_runs"][0];
    assert_eq!((runs["total_count"].as_u64(), run["head_sha"].as_str()), (Some(1), Some(sha.as_str())), "{runs}");
    let jobs = get(format!("actions/runs/{}/jobs", run["id"])).await;
    let mut names: Vec<&str> = jobs["jobs"].as_array().unwrap().iter().map(|job| job["name"].as_str().unwrap()).collect();
    names.sort_unstable();
    assert_eq!(names, ["build", "tests"], "the subset and what it needs, not `lint`: {jobs}");

    // Not a branch.
    let branches = get("branches".to_owned()).await;
    let listed: Vec<&str> = branches.as_array().unwrap().iter().map(|branch| branch["name"].as_str().unwrap()).collect();
    assert_eq!(listed, ["main"], "{branches}");

    // A trailer that names no job starts no run, and the commit says so at once rather than
    // waiting for runs that never come.
    let message = "Try nothing\n\nRbit-CI-Jobs: nope\n";
    let none = git.ok(&sample.dir, &["commit-tree", &tree, "-p", "HEAD", "-m", message]).trim().to_owned();
    git.ok(&sample.dir, &["push", "--quiet", &url, &format!("{none}:refs/scratch/alice/none")]);
    let mut watched = Value::Null;
    for _ in 0..100 {
        watched = get(format!("agent/watch?sha={none}&timeout=1")).await;
        if watched["state"] == "none_expected" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(
        (watched["state"].as_str(), watched["done"].as_bool(), watched["reason"].as_str()),
        (Some("none_expected"), Some(true), Some("no job of the workflows is named by the commit's Rbit-CI-Jobs trailer")),
        "{watched}"
    );
    assert_eq!(get(format!("actions/runs?head_sha={none}")).await["total_count"], 0);

    // Only under one's own name.
    let refused = git.fails(&sample.dir, &["push", &url, &format!("{sha}:refs/scratch/bob/try")]);
    assert!(refused.contains("own scratch"), "{refused}");
}
