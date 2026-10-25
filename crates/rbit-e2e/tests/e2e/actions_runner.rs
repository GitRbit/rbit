//! Actions end to end: a real `rbit-runner` on the host backend registers with a token made
//! on the runners page, takes the jobs of a pushed workflow over the protocol, checks the
//! repository out with its task token, runs bash steps with a secret and step outputs, and
//! reports back — the run, its jobs, the masked log, the job outputs passed to the next job,
//! a skipped `failure()` job and the commit status all come out as on GitHub.
//!
//! The container backend is not exercised here: it needs Docker or Podman, which test hosts
//! may not have. rbit-runner's own tests check every command line it would run.
//!
//! The integration workstream (I) owns this file.

use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use rbit_e2e::{Git, PASSWORD, TestServer, attribute_after};
use serde_json::Value;

/// How long the whole run may take: the runner polls, checks out and runs four short jobs.
const RUN_DEADLINE: Duration = Duration::from_secs(240);

/// The `rbit-runner` built next to this test, by `cargo test --workspace` (the runner's own
/// integration test makes cargo build its binary) or `cargo build -p rbit-runner`.
fn runner_binary() -> PathBuf {
    let exe = std::env::current_exe().expect("the test's own path");
    // target/<profile>/deps/e2e-… → target/<profile>/rbit-runner
    let profile = exe.parent().and_then(std::path::Path::parent).expect("target/<profile>");
    let binary = profile.join(format!("rbit-runner{}", std::env::consts::EXE_SUFFIX));
    assert!(binary.exists(), "{} is missing: run `cargo build -p rbit-runner` (or the whole workspace's tests) first", binary.display());
    binary
}

/// Kills the runner when the test ends, passed or not.
struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

const WORKFLOW: &str = r#"name: CI
on:
  push:
    branches: [main]
jobs:
  build:
    runs-on: self-hosted
    outputs:
      answer: ${{ steps.compute.outputs.answer }}
    steps:
      - uses: actions/checkout@v4
      - name: Look at the checkout
        shell: bash
        run: |
          test -f README.md
          echo "checked out $(git rev-parse HEAD)"
      - id: compute
        name: Compute
        shell: bash
        env:
          GREETING: ${{ secrets.GREETING }}
        run: |
          echo "greeting is $GREETING"
          echo "region is ${{ vars.REGION }}"
          echo "answer=42" >> "$GITHUB_OUTPUT"
  report:
    needs: build
    runs-on: self-hosted
    steps:
      - shell: bash
        run: echo "the answer is ${{ needs.build.outputs.answer }}"
  on-failure:
    needs: build
    if: failure()
    runs-on: self-hosted
    steps:
      - shell: bash
        run: exit 1
"#;

async fn json(client: &reqwest::Client, url: &str) -> Value {
    let response = client.get(url).send().await.expect("the API answers");
    serde_json::from_str(&response.text().await.expect("a body")).expect("JSON")
}

#[tokio::test]
async fn a_real_runner_runs_a_pushed_workflow() {
    let binary = runner_binary();
    let server = TestServer::start_with(|config| config.actions.enabled = true);
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "rocket", "public").await;
    alice.post("/alice/rocket/settings/actions", &[("enabled", "on"), ("token_permission", "read")]).await;
    alice.post("/alice/rocket/settings/actions/secrets", &[("kind", "secret"), ("name", "GREETING"), ("value", "s3cr3t-greeting")]).await;
    alice.post("/alice/rocket/settings/actions/variables", &[("kind", "variable"), ("name", "REGION"), ("value", "eu-west")]).await;

    // A token from the runners page registers a host runner.
    let page = alice.post("/alice/rocket/settings/actions/runners", &[("action", "create")]).await;
    let token = attribute_after(&page.body, r#"id="issued-token""#, "value").expect("the page shows the new token");
    let home = tempfile::tempdir().expect("a runner directory");
    let config = home.path().join("runner.toml");
    let config_arg = config.to_string_lossy().into_owned();
    let output = Command::new(&binary)
        .args([
            "--config",
            &config_arg,
            "register",
            "--url",
            server.base_url(),
            "--insecure-http",
            "--name",
            "e2e",
            "--labels",
            "self-hosted:host",
        ])
        .env("RBIT_RUNNER_TOKEN", &token)
        .output()
        .expect("start rbit-runner register");
    assert!(
        output.status.success(),
        "register failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let runners = alice.get("/alice/rocket/settings/actions/runners").await;
    assert!(runners.contains("e2e") && runners.contains("host — no isolation"));

    // Push the workflow, then let the runner work.
    let git = Git::new();
    let sample = git.sample();
    std::fs::create_dir_all(sample.dir.join(".rbit/workflows")).unwrap();
    let sha = git.commit_file(&sample.dir, ".rbit/workflows/ci.yml", WORKFLOW);
    git.run(&sample.dir, &["push", "--quiet", &server.git_url_as("alice", "rocket", "alice", PASSWORD), "main"]);
    let _runner = Running(
        Command::new(&binary)
            .args(["--config", &config_arg, "run"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start rbit-runner run"),
    );

    let api = reqwest::Client::builder().no_proxy().build().unwrap();
    let runs_url = server.url("/api/v1/repos/alice/rocket/actions/runs");
    let deadline = Instant::now() + RUN_DEADLINE;
    let run = loop {
        let runs = json(&api, &runs_url).await;
        if let Some(run) = runs["workflow_runs"].as_array().and_then(|runs| runs.first()).filter(|run| run["status"] == "completed") {
            break run.clone();
        }
        assert!(Instant::now() < deadline, "the run did not finish in time: {runs}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    assert_eq!(run["head_sha"], sha.as_str());

    let jobs = json(&api, &format!("{runs_url}/{}/jobs", run["id"])).await;
    let conclusion = |name: &str| {
        jobs["jobs"].as_array().unwrap().iter().find(|job| job["name"] == name).map_or(Value::Null, |job| job["conclusion"].clone())
    };
    assert_eq!(conclusion("build"), "success", "{jobs}");
    assert_eq!(conclusion("report"), "success", "{jobs}");
    assert_eq!(conclusion("on-failure"), "skipped", "failure() is false after a success");
    assert_eq!(run["conclusion"], "success");

    // The logs: the checkout worked, the secret is masked, the variable and output arrived.
    let job_id =
        |name: &str| jobs["jobs"].as_array().unwrap().iter().find(|job| job["name"] == name).and_then(|job| job["id"].as_i64()).unwrap();
    let raw = |job: i64| format!("/alice/rocket/actions/runs/{}/jobs/{job}/logs/raw", run["run_number"]);
    let build = alice.get(&raw(job_id("build"))).await.body;
    assert!(build.contains(&format!("checked out {sha}")), "{build}");
    assert!(build.contains("greeting is ***") && !build.contains("s3cr3t-greeting"), "{build}");
    assert!(build.contains("region is eu-west"), "{build}");
    let report = alice.get(&raw(job_id("report"))).await.body;
    assert!(report.contains("the answer is 42"), "{report}");

    // The run reported its jobs as commit statuses.
    let status = json(&api, &server.url(&format!("/api/v1/repos/alice/rocket/commits/{sha}/status"))).await;
    assert_eq!(status["state"], "success", "{status}");
    let page = alice.get(&format!("/alice/rocket/actions/runs/{}/jobs/{}", run["run_number"], job_id("build"))).await;
    assert!(page.contains("Compute") && page.contains("greeting is ***"));
}
