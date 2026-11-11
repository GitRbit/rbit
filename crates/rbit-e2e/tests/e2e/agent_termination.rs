//! Termination records end to end (agent plan step 1.3): a real `rbit-runner` runs a job
//! whose step exits 3 and one that passes, and the jobs API says how each ended: the failed
//! step by name, its exit code and a complete log for the first, `completed` for the second.
//!
//! The integration workstream (I) owns this file.

use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use rbit_e2e::{Git, PASSWORD, TestServer, attribute_after};
use serde_json::Value;

use super::actions_runner::runner_binary;

const WORKFLOW: &str = "name: CI
on:
  push:
    branches: [main]
jobs:
  passes:
    runs-on: self-hosted
    steps:
      - name: Say hello
        shell: bash
        run: echo hello
  fails:
    runs-on: self-hosted
    steps:
      - name: Say hello
        shell: bash
        run: echo hello
      - name: Fail on purpose
        shell: bash
        run: exit 3
";

/// Kills the runner when the test ends, passed or not.
struct Running(std::process::Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn a_failed_job_says_which_step_ended_it_and_with_what_exit_code() {
    let binary = runner_binary();
    let server = TestServer::start_with(|config| config.actions.enabled = true);
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "rocket", "public").await;
    alice.post("/alice/rocket/settings/actions", &[("enabled", "on"), ("token_permission", "read")]).await;

    let page = alice.post("/alice/rocket/settings/actions/runners", &[("action", "create")]).await;
    let token = attribute_after(&page.body, r#"id="issued-token""#, "value").expect("the page shows the new token");
    let home = tempfile::tempdir().expect("a runner directory");
    let config = home.path().join("runner.toml").to_string_lossy().into_owned();
    let url = server.base_url();
    let registered = Command::new(&binary)
        .args(["--config", &config, "register", "--url", url, "--insecure-http", "--name", "e2e", "--labels", "self-hosted:host"])
        .env("RBIT_RUNNER_TOKEN", &token)
        .output()
        .expect("start rbit-runner register");
    assert!(registered.status.success(), "register failed: {}", String::from_utf8_lossy(&registered.stderr));

    let git = Git::new();
    let sample = git.sample();
    std::fs::create_dir_all(sample.dir.join(".rbit/workflows")).unwrap();
    let sha = git.commit_file(&sample.dir, ".rbit/workflows/ci.yml", WORKFLOW);
    git.ok(&sample.dir, &["push", "--quiet", &server.git_url_as("alice", "rocket", "alice", PASSWORD), "main"]);
    let _runner = Running(
        Command::new(&binary).args(["--config", &config, "run"]).stdout(Stdio::null()).stderr(Stdio::null()).spawn().expect("start the runner"),
    );

    let api = reqwest::Client::builder().no_proxy().build().unwrap();
    let get = |path: String| {
        let request = api.get(server.url(&format!("/api/v1/repos/alice/rocket/actions/{path}")));
        async move { serde_json::from_str::<Value>(&request.send().await.unwrap().text().await.unwrap()).unwrap() }
    };
    let deadline = Instant::now() + Duration::from_secs(240);
    let run = loop {
        let runs = get(format!("runs?head_sha={sha}")).await;
        if let Some(run) = runs["workflow_runs"].as_array().and_then(|runs| runs.first()).filter(|run| run["status"] == "completed") {
            break run.clone();
        }
        assert!(Instant::now() < deadline, "the run did not finish in time: {runs}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    };

    let jobs = get(format!("runs/{}/jobs", run["id"])).await;
    let job = |name: &str| jobs["jobs"].as_array().unwrap().iter().find(|job| job["name"] == name).cloned().unwrap();
    let failed = job("fails");
    let ended = &failed["termination"];
    assert_eq!(failed["conclusion"], "failure");
    assert_eq!(ended["reason"], "step_failed", "{failed}");
    assert_eq!(ended["step_name"], "Fail on purpose");
    assert_eq!(ended["exit_code"], 3);
    assert_eq!(ended["log_complete"], true, "a finished job's log is complete");
    let step = failed["steps"].as_array().unwrap().iter().find(|step| step["name"] == "Fail on purpose").unwrap();
    assert_eq!(ended["step"], step["number"], "the step is numbered as in `steps`");

    let passed = job("passes");
    assert_eq!(passed["termination"]["reason"], "completed");
    assert!(passed["termination"]["exit_code"].is_null());
    let one = get(format!("jobs/{}", failed["id"])).await;
    assert_eq!(one["termination"], failed["termination"], "the single-job endpoint says the same");
}
