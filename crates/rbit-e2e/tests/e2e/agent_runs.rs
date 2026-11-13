//! What an agent sees after a push, end to end: runs found by the commit they tested and a
//! branch's runs including its pull request's, stable error codes in API bodies, and the
//! CLI's `--json` objects and exit codes (agent plan step 1.1 and 1.2). No runner is
//! needed: a pushed workflow's runs exist, queued, as soon as the push lands.
//!
//! The integration workstream (I) owns this file.

use std::process::Command;

use rbit_e2e::{Git, PASSWORD, TestServer};
use serde_json::{Value, json};

use super::actions_runner::runner_binary;

const WORKFLOW: &str =
    "name: CI\non: [push, pull_request]\njobs:\n  test:\n    runs-on: nobody-has-this-label\n    steps:\n      - run: true\n";

/// The `rbit` binary built beside `rbit-runner`.
fn rbit_binary() -> std::path::PathBuf {
    let runner = runner_binary();
    let binary = runner.with_file_name(format!("rbit{}", std::env::consts::EXE_SUFFIX));
    assert!(binary.exists(), "{} is missing: build the workspace (`cargo build -p rbit-server`) first", binary.display());
    binary
}

struct Agent<'a> {
    server: &'a TestServer,
    http: reqwest::Client,
    token: String,
    /// The CLI's configuration directory: no stored logins.
    config: tempfile::TempDir,
}

impl Agent<'_> {
    async fn send(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut request = self.http.request(method, self.server.url(&format!("/api/v1/{path}"))).bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.header("Content-Type", "application/json").body(body.to_string());
        }
        let response = request.send().await.expect("the API answers");
        let status = response.status().as_u16();
        let text = response.text().await.expect("a body");
        (status, if text.is_empty() { Value::Null } else { serde_json::from_str(&text).expect("JSON") })
    }

    async fn get(&self, path: &str) -> (u16, Value) {
        self.send(reqwest::Method::GET, path, None).await
    }

    /// `rbit <args> --repo alice/rocket --json`: the exit code and the one JSON object.
    fn cli(&self, args: &[&str]) -> (i32, Value) {
        let output = Command::new(rbit_binary())
            .args(args)
            .args(["--repo", "alice/rocket", "--hostname", self.server.base_url(), "--json"])
            .env("RBIT_TOKEN", &self.token)
            .env_remove("RBIT_REMOTE")
            .env("XDG_CONFIG_HOME", self.config.path())
            .env("APPDATA", self.config.path())
            .output()
            .expect("run rbit");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.lines().collect();
        assert_eq!(lines.len(), 1, "exactly one JSON line on stdout, got:\n{stdout}\nstderr:\n{}", String::from_utf8_lossy(&output.stderr));
        (output.status.code().expect("an exit code"), serde_json::from_str(lines[0]).expect("JSON"))
    }
}

fn shas(runs: &Value) -> Vec<(String, String)> {
    let runs = runs["workflow_runs"].as_array().unwrap_or_else(|| panic!("a list of runs, not {runs}"));
    runs.iter().map(|run| (run["event"].as_str().unwrap().to_owned(), run["head_sha"].as_str().unwrap().to_owned())).collect()
}

#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one scenario, told in order")]
async fn an_agent_finds_the_runs_of_its_commit_and_reads_codes_not_prose() {
    let server = TestServer::start_with(|config| config.actions.enabled = true);
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "rocket", "public").await;
    crate::enable_actions(&alice, "alice", "rocket").await;
    let token = alice.create_token("agent", "write").await;
    let agent = Agent {
        server: &server,
        http: reqwest::Client::builder().no_proxy().build().unwrap(),
        token,
        config: tempfile::tempdir().unwrap(),
    };

    let git = Git::new();
    let sample = git.sample();
    let url = server.git_url_as("alice", "rocket", "alice", PASSWORD);
    std::fs::create_dir_all(sample.dir.join(".rbit/workflows")).unwrap();
    let base = git.commit_file(&sample.dir, ".rbit/workflows/ci.yml", WORKFLOW);
    git.ok(&sample.dir, &["push", "--quiet", &url, "main"]);
    git.ok(&sample.dir, &["checkout", "--quiet", "-b", "agent-change"]);
    let head = git.commit_file(&sample.dir, "feature.txt", "work\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "agent-change"]);
    let (status, pull) = agent
        .send(reqwest::Method::POST, "repos/alice/rocket/pulls", Some(json!({ "title": "Change", "head": "agent-change", "base": "main" })))
        .await;
    assert_eq!(status, 201, "{pull}");

    // By commit: the push run and the pull request run of `head`, nothing of `base`. The
    // pull request's run is started by the event subscriber, so it may come a moment later.
    let mut runs = Value::Null;
    let mut last = 0;
    for _ in 0..100 {
        (last, runs) = agent.get(&format!("repos/alice/rocket/actions/runs?head_sha={head}")).await;
        if runs["total_count"] == 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(runs["total_count"], 2, "the push and pull request runs of the commit, within 10 s: {last} {runs}");
    let mut found = shas(&runs);
    found.sort();
    assert_eq!(found, [("pull_request".to_owned(), head.clone()), ("push".to_owned(), head.clone())]);
    let (_, by_prefix) = agent.get(&format!("repos/alice/rocket/actions/runs?head_sha={}", &head[..7])).await;
    assert_eq!(by_prefix["total_count"], 2);
    let (_, none) = agent.get(&format!("repos/alice/rocket/actions/runs?head_sha={}", "0".repeat(40))).await;
    assert_eq!(none["total_count"], 0, "an unknown commit has no runs: an empty list, not an error");
    let (status, refused) = agent.get("repos/alice/rocket/actions/runs?head_sha=abc").await;
    assert_eq!((status, refused["code"].as_str()), (422, Some("invalid")));

    // A branch's runs include its pull request's (`refs/pull/1/head`).
    let (_, branch) = agent.get("repos/alice/rocket/actions/runs?branch=agent-change").await;
    assert_eq!(shas(&branch).iter().filter(|(event, _)| event == "pull_request").count(), 1, "{branch}");
    let (_, main) = agent.get("repos/alice/rocket/actions/runs?branch=main").await;
    assert_eq!(shas(&main), [("push".to_owned(), base.clone())]);

    // Codes, not prose.
    let (status, missing) = agent.get("repos/alice/nothing-here").await;
    assert_eq!((status, missing["code"].as_str()), (404, Some("not_found")));
    let stale = json!({ "sha": base, "merge_method": "merge" });
    let (status, moved) =
        agent.send(reqwest::Method::PUT, &format!("repos/alice/rocket/pulls/{}/merge", pull["number"]), Some(stale)).await;
    assert_eq!((status, moved["code"].as_str()), (409, Some("head_changed")), "{moved}");

    // The CLI: one JSON object, the exit code of the contract.
    let (code, view) = agent.cli(&["run", "view", "--sha", &head]);
    assert_eq!((code, view["schema"].as_str()), (0, Some("rbit.run/1")), "{view}");
    assert_eq!(view["runs"].as_array().unwrap().len(), 2);
    assert!(view["runs"].as_array().unwrap().iter().all(|run| run["sha"] == head.as_str()));
    let (code, pending) = agent.cli(&["run", "view", "--sha", &"1".repeat(40)]);
    assert_eq!((code, pending["code"].as_str()), (8, Some("pending")), "no run yet is pending, never another commit's run");
    assert!(pending["hint"].as_str().unwrap().starts_with("rbit run watch --sha"));
    let (code, listed) = agent.cli(&["run", "list", "--sha", &head, "--fields", "id,sha"]);
    assert_eq!((code, listed["schema"].as_str(), listed["total"].as_u64()), (0, Some("rbit.runs/1"), Some(2)));
    assert_eq!(listed["runs"][0].as_object().unwrap().len(), 2, "only the asked-for fields");
    let (code, checks) = agent.cli(&["pr", "checks", "1"]);
    assert_eq!((code, checks["state"].as_str()), (8, Some("pending")), "queued checks are pending: {checks}");
    let (code, merge) = agent.cli(&["pr", "merge", "1", "--head", &base]);
    assert_eq!((code, merge["code"].as_str()), (5, Some("head_changed")), "{merge}");
    let (code, gone) = agent.cli(&["run", "view", "999999"]);
    assert_eq!((code, gone["schema"].as_str(), gone["code"].as_str()), (4, Some("rbit.error/1"), Some("not_found")));

    // Where am I: one call.
    let (_, status) = agent.get("repos/alice/rocket/agent/status?branch=agent-change").await;
    assert_eq!(status["schema"], "rbit.status/1");
    assert_eq!((status["head"]["sha"].as_str(), status["ci"]["for_sha"].as_str()), (Some(head.as_str()), Some(head.as_str())));
    assert_eq!(status["ci"]["state"], "queued", "no runner takes the jobs: {status}");
    assert_eq!((status["head"]["divergence"]["ahead"].as_u64(), status["head"]["divergence"]["behind"].as_u64()), (Some(1), Some(0)));
    assert_eq!(status["pr"]["number"], pull["number"]);
    // No runner is registered at all: the jobs would wait for ever, so it says so and
    // suggests no watch.
    let waits = (status["ci"]["reason"].as_str(), status["ci"]["waits_for"][0].as_str());
    assert_eq!(waits, (Some("no_runner"), Some("nobody-has-this-label")), "{status}");
    assert_eq!(status["next"], Value::Null, "{status}");
    // Nothing takes the jobs, so the branch's push job waits behind main's.
    assert_eq!(status["ci"]["queue"]["position"], 2, "{status}");
    let (_, runs) = agent.get(&format!("repos/alice/rocket/actions/runs?head_sha={head}&event=push")).await;
    let (_, jobs) = agent.get(&format!("repos/alice/rocket/actions/runs/{}/jobs", runs["workflow_runs"][0]["id"])).await;
    assert_eq!((jobs["jobs"][0]["queue"]["position"].as_u64(), jobs["jobs"][0]["queue"]["ahead"].as_u64()), (Some(2), Some(1)));
    assert!(serde_json::to_string(&status).unwrap().len() < 1536, "a status is small");
    let (code, cli_status) = agent.cli(&["status", "--branch", "agent-change"]);
    assert_eq!((code, cli_status["schema"].as_str(), cli_status["ci"]["state"].as_str()), (0, Some("rbit.status/1"), Some("queued")));

    // Watch: the first look answers, an unchanged one waits out its timeout.
    let watch = format!("repos/alice/rocket/agent/watch?sha={head}&branch=agent-change&timeout=1");
    let (_, first) = agent.get(&watch).await;
    assert_eq!((first["changed"].as_bool(), first["state"].as_str(), first["done"].as_bool()), (Some(true), Some("queued"), Some(false)));
    let (_, again) = agent.get(&format!("{watch}&cursor={}", first["cursor"].as_str().unwrap())).await;
    assert_eq!(again["changed"], false);
    // `--branch` alone watches the branch's tip on the server, not the checkout's `HEAD` (the
    // test runs inside rbit's own checkout, whose `HEAD` is another commit). The job waits for
    // a label no runner has, so the watch says so at once. A branch the server lacks is a usage error.
    let (code, tip) = agent.cli(&["run", "watch", "--branch", "agent-change", "--timeout", "5"]);
    assert_eq!((code, tip["code"].as_str()), (8, Some("no_runner")), "{tip}");
    assert!(tip["message"].as_str().unwrap().contains(&head[..12]), "the branch's tip is watched: {tip}");
    let (code, unknown_branch) = agent.cli(&["run", "watch", "--branch", "no-such-branch", "--timeout", "5"]);
    assert_eq!((code, unknown_branch["code"].as_str()), (2, Some("usage")), "{unknown_branch}");
    // A commit the repository does not have (yet) is not "no run will come".
    let (_, unknown) = agent.get(&format!("repos/alice/rocket/agent/watch?sha={}&timeout=1", "2".repeat(40))).await;
    assert_eq!((unknown["state"].as_str(), unknown["done"].as_bool()), (Some("not_started"), Some(false)), "{unknown}");
    // A newer push to the branch ends the watch of the older commit.
    let newer = git.commit_file(&sample.dir, "more.txt", "more\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "agent-change"]);
    let (_, moved) = agent.get(&format!("{watch}&cursor={}", first["cursor"].as_str().unwrap())).await;
    assert_eq!(moved["head_changed"]["sha"], newer.as_str(), "{moved}");
    // `--sha` wins over `--branch`: the older commit is watched, not the branch's newer tip.
    let (code, cli_moved) = agent.cli(&["run", "watch", "--sha", &head, "--branch", "agent-change", "--timeout", "5"]);
    assert_eq!((code, cli_moved["code"].as_str()), (5, Some("head_changed")), "{cli_moved}");
    assert!(cli_moved["message"].as_str().unwrap().contains(&head[..12]), "{cli_moved}");

    // A commit with no workflow files expects no run: said at once, not after a wait.
    git.ok(&sample.dir, &["checkout", "--quiet", "-b", "no-ci", "main"]);
    git.ok(&sample.dir, &["rm", "--quiet", "-r", ".rbit"]);
    git.ok(&sample.dir, &["commit", "--quiet", "-m", "Drop CI"]);
    git.ok(&sample.dir, &["push", "--quiet", &url, "no-ci"]);
    let (_, quiet) = agent.get("repos/alice/rocket/agent/status?branch=no-ci").await;
    assert_eq!(
        (quiet["ci"]["state"].as_str(), quiet["ci"]["reason"].as_str()),
        (Some("none_expected"), Some("the commit has no workflow files"))
    );
    // Nothing can change a settled commit, so a watch with its current cursor answers at once
    // rather than after its timeout.
    let quiet_sha = quiet["head"]["sha"].as_str().unwrap().to_owned();
    let (_, first) = agent.get(&format!("repos/alice/rocket/agent/watch?sha={quiet_sha}&timeout=1")).await;
    let started = std::time::Instant::now();
    let settled = format!("repos/alice/rocket/agent/watch?sha={quiet_sha}&timeout=20&cursor={}", first["cursor"].as_str().unwrap());
    let (_, again) = agent.get(&settled).await;
    assert_eq!((again["changed"].as_bool(), again["done"].as_bool()), (Some(false), Some(true)), "{again}");
    assert!(started.elapsed() < std::time::Duration::from_secs(10), "answered at once, not after the timeout");
    // A malformed query gets the API's JSON error.
    let (code, malformed) = agent.get(&format!("repos/alice/rocket/agent/watch?sha={quiet_sha}&timeout=abc")).await;
    assert_eq!((code, malformed["code"].as_str()), (422, Some("invalid")), "{malformed}");

    // Issues as the task queue: one object per command, pull requests left out of lists.
    let (code, created) = agent.cli(&["issue", "create", "--title", "Watch by commit", "--body", "Details"]);
    assert_eq!((code, created["schema"].as_str()), (0, Some("rbit.issue/1")), "{created}");
    let number = created["number"].as_u64().unwrap().to_string();
    let (_, listed) = agent.cli(&["issue", "list"]);
    let numbers: Vec<u64> = listed["issues"].as_array().unwrap().iter().map(|issue| issue["number"].as_u64().unwrap()).collect();
    assert_eq!(numbers, [created["number"].as_u64().unwrap()], "the pull request is not an issue: {listed}");
    let (code, _) = agent.cli(&["issue", "comment", &number, "--body", "Taking it"]);
    assert_eq!(code, 0);
    let (_, viewed) = agent.cli(&["issue", "view", &number]);
    assert_eq!((viewed["body"].as_str(), viewed["comments"]["total"].as_u64()), (Some("Details"), Some(1)));
    assert_eq!(viewed["comments"]["recent"][0]["body"], "Taking it");
    let (code, closed) = agent.cli(&["issue", "close", &number]);
    assert_eq!((code, closed["schema"].as_str()), (0, Some("rbit.issue-closed/1")));
}
