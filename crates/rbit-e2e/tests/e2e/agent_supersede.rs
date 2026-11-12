//! Superseded runs end to end (agent plan step 2.1): with the repository setting on, a newer
//! push to a branch cancels that branch's older run and says which commit superseded it; the
//! default branch and repositories without the setting keep every run. No runner is needed:
//! queued runs are enough.
//!
//! The integration workstream (I) owns this file.

use rbit_e2e::{Git, PASSWORD, TestServer};
use serde_json::{Value, json};

const WORKFLOW: &str = "name: CI\non: [push]\njobs:\n  test:\n    runs-on: nobody-has-this-label\n    steps:\n      - run: true\n";

struct Api {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl Api {
    async fn call(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut request = self.http.request(method, format!("{}/api/v1/repos/alice/rocket/{path}", self.base)).bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.header("Content-Type", "application/json").body(body.to_string());
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, if text.is_empty() { Value::Null } else { serde_json::from_str(&text).unwrap() })
    }

    /// The run of `sha` on `branch`, waiting for the event subscriber to start it.
    async fn run_of(&self, branch: &str, sha: &str) -> Value {
        for _ in 0..100 {
            let (_, runs) = self.call(reqwest::Method::GET, &format!("actions/runs?branch={branch}&per_page=20"), None).await;
            if let Some(run) = runs["workflow_runs"].as_array().unwrap().iter().find(|run| run["head_sha"] == sha) {
                return run.clone();
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        panic!("no run of {sha} on {branch}")
    }
}

#[tokio::test]
async fn a_newer_push_supersedes_a_branchs_older_run_only_when_the_repository_asks() {
    let server = TestServer::start_with(|config| config.actions.enabled = true);
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "rocket", "public").await;
    alice.post("/alice/rocket/settings/actions", &[("enabled", "on"), ("token_permission", "read")]).await;
    let api = Api {
        http: reqwest::Client::builder().no_proxy().build().unwrap(),
        base: server.base_url().to_owned(),
        token: alice.create_token("agent", "admin").await,
    };
    let git = Git::new();
    let sample = git.sample();
    let url = server.git_url_as("alice", "rocket", "alice", PASSWORD);
    std::fs::create_dir_all(sample.dir.join(".rbit/workflows")).unwrap();
    git.commit_file(&sample.dir, ".rbit/workflows/ci.yml", WORKFLOW);
    git.ok(&sample.dir, &["push", "--quiet", &url, "main"]);

    // Off (the default): both runs of the branch stay.
    assert_eq!(api.call(reqwest::Method::GET, "actions/supersede", None).await.1, json!({ "enabled": false }));
    git.ok(&sample.dir, &["checkout", "--quiet", "-b", "topic"]);
    let first = git.commit_file(&sample.dir, "one.txt", "1\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "topic"]);
    let before = git.commit_file(&sample.dir, "two.txt", "2\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "topic"]);
    assert_eq!(api.run_of("topic", &first).await["status"], "queued");
    assert_eq!(api.run_of("topic", &before).await["status"], "queued");

    // On: a run started since then is cancelled by the next push, which names itself. Runs
    // from before the setting are in no group and stay.
    let (status, _) = api.call(reqwest::Method::PUT, "actions/supersede", Some(json!({ "enabled": true }))).await;
    assert_eq!(status, 204);
    let second = git.commit_file(&sample.dir, "three.txt", "3\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "topic"]);
    assert_eq!(api.run_of("topic", &second).await["status"], "queued");
    let third = git.commit_file(&sample.dir, "four.txt", "4\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "topic"]);
    let mut older = api.run_of("topic", &second).await;
    for _ in 0..100 {
        if older["conclusion"] == "cancelled" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        older = api.run_of("topic", &second).await;
    }
    assert_eq!((older["conclusion"].as_str(), older["superseded_by"].as_str()), (Some("cancelled"), Some(third.as_str())), "{older}");
    assert_eq!(api.run_of("topic", &third).await["status"], "queued", "the newest run goes on");
    assert_eq!(api.run_of("topic", &before).await["status"], "queued", "a run from before the setting is in no group");
    let (_, watched) = api.call(reqwest::Method::GET, &format!("agent/watch?sha={second}&timeout=0"), None).await;
    assert_eq!((watched["state"].as_str(), watched["superseded_by"].as_str()), (Some("superseded"), Some(third.as_str())));

    // The default branch keeps every run.
    git.ok(&sample.dir, &["checkout", "--quiet", "main"]);
    let landed = git.commit_file(&sample.dir, "main-one.txt", "1\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "main"]);
    git.commit_file(&sample.dir, "main-two.txt", "2\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "main"]);
    assert_eq!(api.run_of("main", &landed).await["status"], "queued", "runs on the default branch are never superseded");
}
