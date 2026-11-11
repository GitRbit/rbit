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
    let runs = runs["workflow_runs"].as_array().expect("a list of runs");
    runs.iter().map(|run| (run["event"].as_str().unwrap().to_owned(), run["head_sha"].as_str().unwrap().to_owned())).collect()
}

#[tokio::test]
async fn an_agent_finds_the_runs_of_its_commit_and_reads_codes_not_prose() {
    let server = TestServer::start_with(|config| config.actions.enabled = true);
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "rocket", "public").await;
    alice.post("/alice/rocket/settings/actions", &[("enabled", "on"), ("token_permission", "read")]).await;
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
    git.ok(&sample.dir, &["checkout", "--quiet", "-b", "feature"]);
    let head = git.commit_file(&sample.dir, "feature.txt", "work\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "feature"]);
    let (status, pull) = agent
        .send(reqwest::Method::POST, "repos/alice/rocket/pulls", Some(json!({ "title": "Feature", "head": "feature", "base": "main" })))
        .await;
    assert_eq!(status, 201, "{pull}");

    // By commit: the push run and the pull request run of `head`, nothing of `base`.
    let (_, runs) = agent.get(&format!("repos/alice/rocket/actions/runs?head_sha={head}")).await;
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
    let (_, branch) = agent.get("repos/alice/rocket/actions/runs?branch=feature").await;
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
}
