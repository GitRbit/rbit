//! A docs-only change on a protected branch (agent plan step 2.2): the workflow's
//! `paths-ignore` leaves the commit out, the trigger records that, status says
//! `none_expected` with the reason at once, and the required Actions check does not keep
//! the pull request from merging forever. A code change still needs the check.
//!
//! The integration workstream (I) owns this file.

use rbit_e2e::{Git, PASSWORD, TestServer};
use serde_json::{Value, json};

const WORKFLOW: &str = "name: CI
on:
  push:
    paths-ignore: [\"docs/**\"]
jobs:
  test:
    runs-on: nobody-has-this-label
    steps:
      - run: true
";

#[tokio::test]
async fn a_docs_only_change_needs_no_ci_and_still_merges() {
    let server = TestServer::start_with(|config| config.actions.enabled = true);
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "rocket", "public").await;
    alice.post("/alice/rocket/settings/actions", &[("enabled", "on"), ("token_permission", "read")]).await;
    let token = alice.create_token("agent", "admin").await;
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let call = |method: reqwest::Method, path: String, body: Option<Value>| {
        let mut request = http.request(method, server.url(&format!("/api/v1/repos/alice/rocket/{path}"))).bearer_auth(&token);
        if let Some(body) = body {
            request = request.header("Content-Type", "application/json").body(body.to_string());
        }
        async move {
            let response = request.send().await.unwrap();
            let status = response.status().as_u16();
            let text = response.text().await.unwrap();
            (status, if text.is_empty() { Value::Null } else { serde_json::from_str::<Value>(&text).unwrap() })
        }
    };

    let git = Git::new();
    let sample = git.sample();
    let url = server.git_url_as("alice", "rocket", "alice", PASSWORD);
    std::fs::create_dir_all(sample.dir.join(".rbit/workflows")).unwrap();
    git.commit_file(&sample.dir, ".rbit/workflows/ci.yml", WORKFLOW);
    git.ok(&sample.dir, &["push", "--quiet", &url, "main"]);
    let protection = json!({ "pattern": "main", "require_pull_request": true, "required_approvals": 0, "require_status_checks": true,
        "required_status_checks": ["CI / test (push)"] });
    let (status, _) = call(reqwest::Method::POST, "branch_protections".to_owned(), Some(protection)).await;
    assert_eq!(status, 201);

    // Docs only: no run, and status says why at once.
    git.ok(&sample.dir, &["checkout", "--quiet", "-b", "docs-change"]);
    std::fs::create_dir_all(sample.dir.join("docs")).unwrap();
    let docs = git.commit_file(&sample.dir, "docs/guide.md", "# Guide\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "docs-change"]);
    // What the push started is recorded just after git answers: wait for that decision.
    let mut state = Value::Null;
    for _ in 0..50 {
        state = call(reqwest::Method::GET, "agent/status?branch=docs-change".to_owned(), None).await.1;
        if !state["ci"]["state"].is_null() && state["ci"]["state"] != "not_started" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(state["ci"]["state"], "none_expected", "{state}");
    assert!(state["ci"]["reason"].as_str().unwrap().contains("filters"), "{state}");

    // The required check is not required of it: the pull request merges.
    let pull = json!({ "title": "Docs", "head": "docs-change", "base": "main" });
    let (_, opened) = call(reqwest::Method::POST, "pulls".to_owned(), Some(pull)).await;
    let merge = json!({ "sha": docs, "merge_method": "merge" });
    let (status, merged) = call(reqwest::Method::PUT, format!("pulls/{}/merge", opened["number"]), Some(merge)).await;
    assert_eq!(status, 200, "a docs-only change is not held by CI that will never run: {merged}");

    // A code change still needs the check.
    git.ok(&sample.dir, &["checkout", "--quiet", "-b", "code-change", "main"]);
    let code = git.commit_file(&sample.dir, "src.txt", "code\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "code-change"]);
    let pull = json!({ "title": "Code", "head": "code-change", "base": "main" });
    let (_, opened) = call(reqwest::Method::POST, "pulls".to_owned(), Some(pull)).await;
    let merge = json!({ "sha": code, "merge_method": "merge" });
    let (status, blocked) = call(reqwest::Method::PUT, format!("pulls/{}/merge", opened["number"]), Some(merge)).await;
    assert_eq!((status, blocked["code"].as_str()), (405, Some("blocked")), "{blocked}");
}
