//! Exclusive runner labels end to end (agent plan step 2.3): a runner with room for two
//! jobs and an exclusive `bench` label never runs the `bench` job beside another one, while
//! its ordinary jobs still run two at a time.
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
  one:
    runs-on: self-hosted
    steps:
      - shell: bash
        run: sleep 3
  two:
    runs-on: self-hosted
    steps:
      - shell: bash
        run: sleep 3
  bench:
    runs-on: bench
    steps:
      - shell: bash
        run: sleep 3
";

struct Running(std::process::Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn seconds(job: &Value, key: &str) -> i64 {
    let text = job[key].as_str().unwrap_or_else(|| panic!("{key} of {job}"));
    // `2026-11-11T10:00:03Z`: the API's resolution is a second.
    let time = &text[11..19];
    let parts: Vec<i64> = time.split(':').map(|part| part.parse().unwrap()).collect();
    parts[0] * 3600 + parts[1] * 60 + parts[2]
}

#[tokio::test]
async fn an_exclusive_job_has_the_runner_to_itself() {
    let binary = runner_binary();
    let server = TestServer::start_with(|config| config.actions.enabled = true);
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "rocket", "public").await;
    alice.post("/alice/rocket/settings/actions", &[("enabled", "on"), ("token_permission", "read")]).await;
    let page = alice.post("/alice/rocket/settings/actions/runners", &[("action", "create")]).await;
    let token = attribute_after(&page.body, r#"id="issued-token""#, "value").expect("a registration token");
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("runner.toml");
    let config_arg = config.to_string_lossy().into_owned();
    let registered = Command::new(&binary)
        .args(["--config", &config_arg, "register", "--url", server.base_url(), "--insecure-http", "--name", "e2e"])
        .args(["--labels", "self-hosted:host,bench:host", "--capacity", "2"])
        .env("RBIT_RUNNER_TOKEN", &token)
        .output()
        .unwrap();
    assert!(registered.status.success(), "{}", String::from_utf8_lossy(&registered.stderr));
    let written = std::fs::read_to_string(&config).unwrap();
    assert!(written.contains("name = \"bench\""), "{written}");
    std::fs::write(&config, written.replace("name = \"bench\"", "name = \"bench\"\nexclusive = true")).unwrap();

    let git = Git::new();
    let sample = git.sample();
    std::fs::create_dir_all(sample.dir.join(".rbit/workflows")).unwrap();
    let sha = git.commit_file(&sample.dir, ".rbit/workflows/ci.yml", WORKFLOW);
    git.ok(&sample.dir, &["push", "--quiet", &server.git_url_as("alice", "rocket", "alice", PASSWORD), "main"]);
    // The runner's own log is kept, so a run that never finishes says why.
    let log_path = home.path().join("runner.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let _runner = Running(Command::new(&binary).args(["--config", &config_arg, "run"]).stdout(Stdio::null()).stderr(log).spawn().unwrap());

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
        if Instant::now() >= deadline {
            let jobs = get(format!("runs/{}/jobs", runs["workflow_runs"][0]["id"])).await;
            let log = std::fs::read_to_string(&log_path).unwrap_or_default();
            let tail: Vec<&str> = log.lines().rev().take(40).collect::<Vec<_>>().into_iter().rev().collect();
            panic!("the run did not finish: {runs}\njobs: {jobs}\nrunner log:\n{}", tail.join("\n"));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    assert_eq!(run["conclusion"], "success", "{run}");
    let jobs = get(format!("runs/{}/jobs", run["id"])).await;
    let job = |name: &str| jobs["jobs"].as_array().unwrap().iter().find(|job| job["name"] == name).cloned().unwrap();
    let (bench, one, two) = (job("bench"), job("one"), job("two"));
    let span = |job: &Value| (seconds(job, "started_at"), seconds(job, "completed_at"));
    let (bench_start, bench_end) = span(&bench);
    for other in [&one, &two] {
        let (start, end) = span(other);
        assert!(end <= bench_start || start >= bench_end, "the bench job ran beside {}: {jobs}", other["name"]);
    }
    let ((one_start, one_end), (two_start, two_end)) = (span(&one), span(&two));
    assert!(one_start < two_end && two_start < one_end, "ordinary jobs still share the runner: {jobs}");
}
