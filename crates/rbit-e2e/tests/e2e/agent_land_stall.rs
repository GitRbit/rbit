//! A queued pull request whose required check can never report on the queue's candidate
//! leaves the line with the reason, instead of holding up every entry behind it.
//!
//! The integration workstream (I) owns this file.

use rbit_e2e::{Git, PASSWORD, TestServer};
use reqwest::{Method, StatusCode};
use serde_json::json;

use crate::agent_land::Api;

#[tokio::test]
async fn an_entry_whose_required_check_cannot_report_leaves_the_line() {
    let server = TestServer::start_with(|config| config.actions.enabled = true);
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "rocket", "public").await;
    crate::enable_actions(&alice, "alice", "rocket").await;
    let api = Api::new(&server, alice.create_token("agent", "admin").await);

    // main requires a check that only pull request runs report; the repository has no workflow.
    let git = Git::new();
    let sample = git.sample();
    let url = server.git_url_as("alice", "rocket", "alice", PASSWORD);
    git.commit_file(&sample.dir, "rocket.txt", "v1\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "main"]);
    let protection = json!({ "pattern": "main", "require_pull_request": true, "required_approvals": 0, "require_status_checks": true,
        "required_status_checks": ["CI / test (pull_request)"] });
    let (status, rule) = api.send(Method::POST, "branch_protections", Some(protection)).await;
    assert_eq!(status, StatusCode::CREATED, "{rule}");
    git.ok(&sample.dir, &["checkout", "--quiet", "-b", "landing"]);
    let head = git.commit_file(&sample.dir, "landing.txt", "legs\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "landing"]);
    let (status, pull) = api.send(Method::POST, "pulls", Some(json!({ "title": "Landing legs", "head": "landing", "base": "main" }))).await;
    assert_eq!(status, StatusCode::CREATED, "{pull}");
    let number = pull["number"].as_u64().unwrap();

    // The candidate is tested by a push, so a pull request run's check never comes.
    let queue = format!("pulls/{number}/queue");
    let (status, place) = api.send(Method::POST, &queue, Some(json!({ "head": head }))).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{place}");
    let left = api.queue_until(number, |entry| entry["state"] == "failed").await;
    let reason = left["reason"].as_str().unwrap_or_default();
    assert!(reason.contains("`CI / test (pull_request)` is reported by pull_request runs"), "{left}");

    // Required from push runs instead: no workflow runs on the candidate, so it leaves too.
    let checks = json!({ "required_status_checks": ["CI / test (push)"] });
    let (status, changed) = api.send(Method::PATCH, &format!("branch_protections/{}", rule["id"]), Some(checks)).await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    let (status, place) = api.send(Method::POST, &queue, Some(json!({ "head": head }))).await;
    assert_eq!(status, StatusCode::ACCEPTED, "queued again after leaving: {place}");
    let left = api.queue_until(number, |entry| entry["state"] == "failed" && entry["reason"] != reason).await;
    assert!(left["reason"].as_str().unwrap_or_default().contains("no workflow ran on the candidate"), "{left}");
    let (_, main) = api.send(Method::GET, "branches/main", None).await;
    assert_ne!(main["commit"]["sha"].as_str(), left["candidate"].as_str(), "nothing landed");
}
