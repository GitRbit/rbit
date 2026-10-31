//! Actions end to end on the container backend: a real `rbit-runner` with a Docker label
//! takes the jobs of a pushed workflow and runs them in containers — checkout, a matrix,
//! job outputs, a masked secret, `GITHUB_ENV`, cache, artifacts, a local composite, a
//! JavaScript action on the runner's Node.js and a `docker://` action — and the job
//! container sees neither the engine's socket nor the runner's credential. A second
//! workflow fails and turns the commit status red; a third, dispatched twice in one
//! `cancel-in-progress` group, cancels a Docker action that ignores `SIGTERM`, and nothing
//! it started is left running.
//!
//! It needs Docker (or Podman) that can pull `ubuntu:24.04` and `alpine:3.20`, so it runs
//! only when `RBIT_E2E_DOCKER=1`; the host-backend test (`actions_runner`) runs everywhere.

use std::{
    fmt::Write as _,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use rbit_e2e::{Git, PASSWORD, TestServer, attribute_after};
use serde_json::{Value, json};

use super::actions_runner::runner_binary;

/// Pulling two images and running three workflows' containers.
const RUN_DEADLINE: Duration = Duration::from_mins(10);

const RUNNER: &str = "e2e-docker";

struct Running(std::process::Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

const CI: &str = r#"name: CI
on:
  push:
    branches: [main]
env:
  TOP: top-level
jobs:
  build:
    runs-on: linux
    outputs:
      answer: ${{ steps.compute.outputs.answer }}
    steps:
      - uses: actions/checkout@v4
      - id: compute
        name: Compute
        env:
          GREETING: ${{ secrets.GREETING }}
        run: |
          test -f README.md
          grep '^ID=' /etc/os-release
          echo "checked out $(cat marker.txt)"
          echo "greeting is $GREETING"
          echo "top is $TOP"
          echo "answer=42" >> "$GITHUB_OUTPUT"
          echo "CARRIED=from-env-file" >> "$GITHUB_ENV"
      - run: echo "carried is $CARRIED"
      - name: Probe the container
        run: |
          for socket in /var/run/docker.sock /run/docker.sock /run/podman/podman.sock; do
            if [ -e "$socket" ]; then echo "ESC""APE: $socket is in the job container"; fi
          done
          if env | grep -E '^(RBIT_RUNNER|DOCKER_HOST|CONTAINER_HOST)'; then echo "ESC""APE: runner environment"; fi
          ls -a /__w
          echo "workspace is $GITHUB_WORKSPACE"
          # A link out of the workspace, for the host-side upload to (not) follow.
          ln -s ../../../../.credential dist-credential || true
          mkdir -p dist && echo "built at $GITHUB_SHA" > dist/app.txt
          LINK_IN_ARTIFACT
      - uses: actions/cache@v4
        with:
          key: e2e-cache-${{ github.sha }}
          path: dist
      - uses: actions/upload-artifact@v4
        with:
          name: dist
          path: dist
      - name: An artifact path out of the workspace is refused
        id: outside
        continue-on-error: true
        uses: actions/upload-artifact@v4
        with:
          name: outside
          path: ../../..
      - run: echo "outside upload outcome ${{ steps.outside.outcome }}"
      - uses: ./.rbit/actions/greet
        with:
          who: composite
      - uses: ./.rbit/actions/hello-js
        id: js
        with:
          who: javascript
      - run: echo "js said ${{ steps.js.outputs.said }}"
      - uses: docker://docker.io/library/alpine:3.20
        with:
          entrypoint: /bin/sh
          args: -c "echo docker action sees $GITHUB_WORKSPACE; ls /github/workspace/README.md"
  matrix:
    runs-on: linux
    strategy:
      matrix:
        flavour: [plain, salted]
        include:
          - flavour: salted
            salt: nacl
    steps:
      - run: echo "flavour ${{ matrix.flavour }} salt '${{ matrix.salt }}'"
  report:
    needs: [build, matrix]
    runs-on: linux
    steps:
      - uses: actions/download-artifact@v4
        with:
          name: dist
      - run: |
          echo "the answer is ${{ needs.build.outputs.answer }}"
          cat app.txt
          ls -la
          cat credential 2>&1 || true
  on-failure:
    needs: build
    if: failure()
    runs-on: linux
    steps:
      - run: exit 1
"#;

const FAIL: &str = r#"name: Fails
on:
  push:
    branches: [main]
jobs:
  broken:
    runs-on: linux
    steps:
      - run: echo "about to fail"; exit 3
      - run: echo "never printed"
"#;

const SLOW: &str = r#"name: Slow
on:
  workflow_dispatch:
    inputs:
      seconds:
        type: number
        default: 1
concurrency:
  group: slow
  cancel-in-progress: true
jobs:
  sleep:
    runs-on: linux
    steps:
      - uses: docker://docker.io/library/alpine:3.20
        with:
          entrypoint: /bin/sh
          args: -c "trap '' TERM; echo rbit-e2e-sleeping; sleep ${{ inputs.seconds }}"
"#;

const GREET: &str = r#"name: Greet
inputs:
  who:
    required: true
runs:
  using: composite
  steps:
    - shell: bash
      run: echo "hello from ${{ inputs.who }}"
"#;

const HELLO_JS: &str = r"name: Hello
inputs:
  who:
    required: true
outputs:
  said:
    description: what it said
runs:
  using: node20
  main: index.js
";

const HELLO_JS_MAIN: &str = r"const fs = require('fs');
const who = process.env['INPUT_WHO'];
console.log(`hello from ${who} on node ${process.version}`);
fs.appendFileSync(process.env['GITHUB_OUTPUT'], `said=hi-${who}\n`);
";

async fn json(client: &reqwest::Client, url: &str) -> Value {
    let response = client.get(url).send().await.expect("the API answers");
    serde_json::from_str(&response.text().await.expect("a body")).expect("JSON")
}

/// The runs of `workflow` (`name`), newest first.
async fn runs_of(api: &reqwest::Client, server: &TestServer, workflow: &str) -> Vec<Value> {
    let runs = json(api, &server.url("/api/v1/repos/alice/probe/actions/runs?per_page=100")).await;
    runs["workflow_runs"].as_array().cloned().unwrap_or_default().into_iter().filter(|run| run["name"] == workflow).collect()
}

async fn wait_for<F: Fn(&[Value]) -> bool>(api: &reqwest::Client, server: &TestServer, workflow: &str, done: F) -> Vec<Value> {
    let deadline = Instant::now() + RUN_DEADLINE;
    loop {
        let runs = runs_of(api, server, workflow).await;
        if done(&runs) {
            return runs;
        }
        assert!(Instant::now() < deadline, "`{workflow}` did not get there in time: {runs:?}");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn completed(run: &Value) -> bool {
    run["status"] == "completed"
}

/// Containers of the engine whose command mentions `needle`, running or not.
fn containers_running(needle: &str) -> Vec<String> {
    let output =
        Command::new("docker").args(["ps", "--all", "--no-trunc", "--format", "{{.ID}} {{.Command}}"]).output().expect("docker ps");
    String::from_utf8_lossy(&output.stdout).lines().filter(|line| line.contains(needle)).map(str::to_owned).collect()
}

#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one scenario, read top to bottom")]
async fn a_container_runner_runs_a_pushed_workflow() {
    if std::env::var("RBIT_E2E_DOCKER").as_deref() != Ok("1") {
        eprintln!("skipped: set RBIT_E2E_DOCKER=1 to run the container backend end to end");
        return;
    }
    let binary = runner_binary();
    let server = TestServer::start_with(|config| config.actions.enabled = true);
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "probe", "public").await;
    alice.post("/alice/probe/settings/actions", &[("enabled", "on"), ("token_permission", "read")]).await;
    alice.post("/alice/probe/settings/actions/secrets", &[("kind", "secret"), ("name", "GREETING"), ("value", "s3cr3t-greeting")]).await;
    let token = alice.create_token("e2e", "write").await;

    let page = alice.post("/alice/probe/settings/actions/runners", &[("action", "create")]).await;
    let registration = attribute_after(&page.body, r#"id="issued-token""#, "value").expect("the page shows the new token");
    let home = tempfile::tempdir().expect("a runner directory");
    let config = home.path().join("runner.toml");
    let config_arg = config.to_string_lossy().into_owned();
    let output = Command::new(&binary)
        .args(["--config", &config_arg, "register", "--url", server.base_url(), "--insecure-http", "--name", RUNNER])
        .args(["--labels", "linux:container:docker.io/library/ubuntu:24.04"])
        .env("RBIT_RUNNER_TOKEN", &registration)
        .output()
        .expect("start rbit-runner register");
    assert!(
        output.status.success(),
        "register failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let credential = std::fs::read_to_string(home.path().join(".credential")).expect("the runner's credential").trim().to_owned();

    let git = Git::new();
    let sample = git.sample();
    for (path, contents) in [
        // A link made in a Linux container is a WSL reparse point to a Windows host, which
        // the upload refuses outright; elsewhere it is packed as a link, never followed.
        (
            ".rbit/workflows/ci.yml",
            CI.replace("LINK_IN_ARTIFACT", if cfg!(windows) { "true" } else { "ln -s ../../../../../.credential dist/credential" }),
        ),
        ("marker.txt", "probe-marker\n".to_owned()),
        (".rbit/workflows/fail.yml", FAIL.to_owned()),
        (".rbit/workflows/slow.yml", SLOW.to_owned()),
        (".rbit/actions/greet/action.yml", GREET.to_owned()),
        (".rbit/actions/hello-js/action.yml", HELLO_JS.to_owned()),
        (".rbit/actions/hello-js/index.js", HELLO_JS_MAIN.to_owned()),
    ] {
        std::fs::create_dir_all(sample.dir.join(path).parent().unwrap()).unwrap();
        std::fs::write(sample.dir.join(path), contents).unwrap();
    }
    git.ok(&sample.dir, &["add", "."]);
    git.ok(&sample.dir, &["commit", "--quiet", "-m", "Add workflows"]);
    let sha = git.ok(&sample.dir, &["rev-parse", "HEAD"]).trim().to_owned();
    git.run(&sample.dir, &["push", "--quiet", &server.git_url_as("alice", "probe", "alice", PASSWORD), "main"]);

    let log = std::fs::File::create(home.path().join("runner.log")).unwrap();
    let _runner = Running(
        Command::new(&binary)
            .args(["--config", &config_arg, "run"])
            .env("RUST_LOG", "info")
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .expect("start rbit-runner run"),
    );
    let api = reqwest::Client::builder().no_proxy().build().unwrap();
    let runner_log = || std::fs::read_to_string(home.path().join("runner.log")).unwrap_or_default();

    let ci = wait_for(&api, &server, "CI", |runs| runs.first().is_some_and(completed)).await.remove(0);
    let fail = wait_for(&api, &server, "Fails", |runs| runs.first().is_some_and(completed)).await.remove(0);
    let jobs = json(&api, &server.url(&format!("/api/v1/repos/alice/probe/actions/runs/{}/jobs", ci["id"]))).await;
    let jobs = jobs["jobs"].as_array().cloned().unwrap_or_default();
    let raw = |run: &Value, job: &Value| format!("/alice/probe/actions/runs/{}/jobs/{}/logs/raw", run["run_number"], job["id"]);
    let mut logs = String::new();
    for job in jobs.iter().filter(|job| job["conclusion"] != "skipped") {
        let _ = writeln!(logs, "===== {} ({})", job["name"], job["conclusion"]);
        logs.push_str(&alice.get(&raw(&ci, job)).await.body);
    }
    let conclusion = |name: &str| jobs.iter().find(|job| job["name"] == name).map_or(Value::Null, |job| job["conclusion"].clone());
    assert_eq!(conclusion("build"), "success", "{logs}\n{}", runner_log());
    assert_eq!(conclusion("report"), "success", "{logs}");
    assert_eq!(conclusion("on-failure"), "skipped", "{logs}");
    assert_eq!(ci["conclusion"], "success", "{logs}");
    let matrix: Vec<_> = jobs.iter().filter(|job| job["name"].as_str().is_some_and(|name| name.starts_with("matrix"))).collect();
    assert_eq!(matrix.len(), 2, "{jobs:?}");

    for expected in [
        "ID=ubuntu",
        "checked out probe-marker",
        "greeting is ***",
        "top is top-level",
        "carried is from-env-file",
        "workspace is /__w/probe",
        "outside upload outcome failure",
        "hello from composite",
        "hello from javascript on node v",
        "js said hi-javascript",
        "docker action sees /github/workspace",
        "flavour plain salt ''",
        "flavour salted salt 'nacl'",
        "the answer is 42",
        &format!("built at {sha}"),
    ] {
        assert!(logs.contains(expected), "missing `{expected}`:\n{logs}");
    }
    assert!(!logs.contains("s3cr3t-greeting"), "the secret leaked:\n{logs}");
    assert!(!logs.contains("ESCAPE"), "the job container reached the host:\n{logs}");
    assert!(!logs.contains(&credential), "the runner credential reached a log:\n{logs}");

    // The failing workflow: its job failed at the right step and the commit is red.
    let fail_jobs = json(&api, &server.url(&format!("/api/v1/repos/alice/probe/actions/runs/{}/jobs", fail["id"]))).await;
    let broken = &fail_jobs["jobs"][0];
    assert_eq!((fail["conclusion"].clone(), broken["conclusion"].clone()), (json!("failure"), json!("failure")));
    let broken_log = alice.get(&raw(&fail, broken)).await.body;
    assert!(broken_log.contains("about to fail") && !broken_log.contains("never printed"), "{broken_log}");
    let status = json(&api, &server.url(&format!("/api/v1/repos/alice/probe/commits/{sha}/status"))).await;
    assert_eq!(status["state"], "failure", "{status}");

    // Two dispatches in one cancel-in-progress group: the first, a Docker action that
    // ignores SIGTERM, is cancelled while it sleeps, and its container goes with it.
    let dispatch = server.url("/api/v1/repos/alice/probe/actions/workflows/slow.yml/dispatches");
    let send = |seconds: &str| {
        api.post(&dispatch)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(json!({"ref": "main", "inputs": {"seconds": seconds}}).to_string())
            .send()
    };
    let response = send("601").await.expect("dispatch");
    assert!(response.status().is_success(), "{}", response.text().await.unwrap_or_default());
    let deadline = Instant::now() + RUN_DEADLINE;
    while containers_running("sleep 601").is_empty() {
        assert!(Instant::now() < deadline, "the slow Docker action never started:\n{}", runner_log());
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let response = send("1").await.expect("dispatch");
    assert!(response.status().is_success());
    let slow = wait_for(&api, &server, "Slow", |runs| runs.len() == 2 && runs.iter().all(completed)).await;
    assert_eq!(slow[1]["conclusion"], "cancelled", "{slow:?}");
    assert_eq!(slow[0]["conclusion"], "success", "{slow:?}");
    let cancelled_jobs = json(&api, &server.url(&format!("/api/v1/repos/alice/probe/actions/runs/{}/jobs", slow[1]["id"]))).await;
    let cancelled_log = alice.get(&raw(&slow[1], &cancelled_jobs["jobs"][0])).await.body;
    assert!(cancelled_log.contains("rbit-e2e-sleeping"), "the cancelled job's log kept its output: {cancelled_log}");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let left = containers_running("sleep 601");
        if left.is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "the cancelled Docker action is still there: {left:?}\n{}", runner_log());
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
