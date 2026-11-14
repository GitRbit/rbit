//! A PR watch follows collaboration as well as CI for the observed head.

use std::time::{Duration, Instant};

use rbit_e2e::{Git, PASSWORD, TestServer};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use super::agent_land::Api;

struct PullWatch<'a> {
    api: &'a Api,
    path: String,
}

impl PullWatch<'_> {
    async fn snapshot(&self, previous: Option<&Value>, timeout: u64) -> Value {
        let after = previous.map_or_else(String::new, |snapshot| format!("&cursor={}", snapshot["cursor"].as_str().unwrap()));
        let path = format!("{}/watch?timeout={timeout}{after}", self.path);
        let (status, body) = self.api.send(Method::GET, &path, None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["schema"], "rbit.pr-watch/1", "{body}");
        body
    }

    async fn changed(&self, previous: &Value) -> Value {
        let latest = self.snapshot(Some(previous), 5).await;
        assert_eq!(latest["changed"], true, "{latest}");
        assert_ne!(latest["cursor"], previous["cursor"], "{latest}");
        latest
    }
}

fn check_state(snapshot: &Value) -> &Value {
    let check = snapshot["readiness"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["context"] == "ci/test")
        .expect("the configured required check is reported");
    assert_eq!(check["required"], true, "{snapshot}");
    &check["state"]
}

async fn revocation_ends_watch(server: &TestServer, owner_token: &str, pull_path: &str) {
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let tokens_url = server.url("/api/v1/user/tokens");
    let created = http
        .post(&tokens_url)
        .bearer_auth(owner_token)
        .json(&json!({ "name": "revoked watch", "scope": "read", "repository": "alice/rocket" }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created: Value = created.json().await.unwrap();
    let id = created["id"].as_i64().unwrap();
    let reader = Api::new(server, created["token"].as_str().unwrap().to_owned());
    let watch = PullWatch { api: &reader, path: pull_path.to_owned() };
    let snapshot = watch.snapshot(None, 0).await;
    let cursor = snapshot["cursor"].as_str().unwrap();
    let watch_path = format!("{pull_path}/watch?timeout=8&cursor={cursor}");
    let (answer, ()) = tokio::join!(reader.send(Method::GET, &watch_path, None), async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let revoked = http.delete(format!("{tokens_url}/{id}")).bearer_auth(owner_token).send().await.unwrap();
        assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
    });
    let (status, body) = answer;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "the waiting request cannot return repository data after revocation: {body}");
}

#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one pull request is observed through each successive change")]
async fn agent_pr_watch_reports_checks_reviews_and_revisions() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    let outsider = server.sign_up("outsider").await;
    alice.create_repo("alice", "rocket", "private").await;
    let owner_token = alice.create_token("owner", "admin").await;
    let owner = Api::new(&server, owner_token.clone());
    let reader = Api::new(&server, alice.create_token("read-only watch", "read").await);
    let reviewer = Api::new(&server, bob.create_token("review", "write").await);
    let outsider = Api::new(&server, outsider.create_token("outsider", "read").await);
    let (status, body) = owner.send(Method::PUT, "collaborators/bob", Some(json!({ "permission": "push" }))).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let git = Git::new();
    let sample = git.sample();
    let remote = server.git_url_as("alice", "rocket", "alice", PASSWORD);
    let base = git.commit_file(&sample.dir, "rocket.txt", "v1\n");
    git.ok(&sample.dir, &["push", "--quiet", &remote, "main"]);
    let protection = json!({ "pattern": "main", "require_status_checks": true, "required_status_checks": ["ci/test"] });
    assert_eq!(owner.send(Method::POST, "branch_protections", Some(protection)).await.0, StatusCode::CREATED);
    git.ok(&sample.dir, &["checkout", "--quiet", "-b", "landing"]);
    let head = git.commit_file(&sample.dir, "landing.txt", "legs\n");
    git.ok(&sample.dir, &["push", "--quiet", &remote, "landing"]);
    let request = json!({ "title": "Landing legs", "head": "landing", "base": "main" });
    let (status, pull) = owner.send(Method::POST, "pulls", Some(request)).await;
    assert_eq!(status, StatusCode::CREATED, "{pull}");
    let path = format!("pulls/{}", pull["number"]);
    assert_eq!(outsider.send(Method::GET, &format!("{path}/watch?timeout=0"), None).await.0, StatusCode::NOT_FOUND);
    let read_watch = PullWatch { api: &reader, path: path.clone() };
    let read_snapshot = read_watch.snapshot(None, 0).await;
    assert_eq!(read_snapshot["head"], head, "{read_snapshot}");
    assert_eq!(read_snapshot["readiness_checks_protection"], false, "{read_snapshot}");
    assert_eq!(read_snapshot["can_request_merge"], false, "{read_snapshot}");
    let watch = PullWatch { api: &owner, path };

    let first = watch.snapshot(None, 0).await;
    assert_eq!(first["changed"], true, "{first}");
    assert_eq!(first["state"], "open", "{first}");
    assert_eq!(first["head"], head, "{first}");
    assert_eq!(first["base_sha"], base, "{first}");
    assert_eq!(first["readiness_checks_protection"], true, "{first}");
    assert_eq!(first["can_request_merge"], true, "{first}");
    assert_eq!(check_state(&first), "expected");

    let status_path = format!("statuses/{head}");
    let report = |state| owner.send(Method::POST, &status_path, Some(json!({ "state": state, "context": "ci/test" })));
    let (status, body) = report("failure").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let failed = watch.changed(&first).await;
    assert_eq!(check_state(&failed), "failed");
    let (status, body) = report("success").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let mut green = watch.changed(&failed).await;
    assert_eq!(check_state(&green), "passed");
    // Allow the background conflict check to settle before testing an unchanged cursor.
    let deadline = Instant::now() + Duration::from_secs(15);
    while green["readiness"]["ready"] != true {
        assert!(Instant::now() < deadline, "the conflict check did not settle: {green}");
        green = watch.snapshot(Some(&green), 1).await;
    }
    let started = Instant::now();
    let unchanged = watch.snapshot(Some(&green), 1).await;
    assert_eq!(unchanged["changed"], false, "{unchanged}");
    assert_eq!(unchanged["cursor"], green["cursor"], "{unchanged}");
    assert!(started.elapsed() >= Duration::from_secs(1), "an unchanged cursor waited for the requested time");
    revocation_ends_watch(&server, &owner_token, &watch.path).await;

    let reviews = format!("{}/reviews", watch.path);
    let comments = format!("{}/comments", watch.path);
    let pending = json!({ "comments": [{ "path": "landing.txt", "line": 1, "body": "Pending and private" }] });
    let (status, body) = reviewer.send(Method::POST, &reviews, Some(pending)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let invisible = watch.snapshot(Some(&green), 0).await;
    assert_eq!(invisible["changed"], false, "pending review data remains private: {invisible}");

    let submitted = json!({ "event": "COMMENT", "body": "Please clarify this" });
    let (reviewed, _) = tokio::join!(watch.changed(&green), async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let (status, body) = reviewer.send(Method::POST, &reviews, Some(submitted)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    });
    assert_eq!(check_state(&reviewed), "passed");
    assert_eq!(reviewed["reviews"].as_array().unwrap().len(), 1, "{reviewed}");
    assert_eq!(reviewed["reviews"][0]["state"], "commented", "{reviewed}");
    assert_eq!(reviewed["reviews"][0]["commit"], head, "{reviewed}");
    let root = reviewed["threads"][0]["id"].as_i64().expect("a submitted review thread");
    let reply = json!({ "in_reply_to": root, "body": "The legs deploy before contact." });
    let (status, body) = owner.send(Method::POST, &comments, Some(reply)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let replied = watch.changed(&reviewed).await;
    assert_eq!(replied["threads"][0]["replies"], 1, "{replied}");
    let resolution = format!("{comments}/{root}/resolution");
    let (status, body) = reviewer.send(Method::PUT, &resolution, Some(json!({ "resolved": true }))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let resolved = watch.changed(&replied).await;
    assert_eq!(resolved["threads"], json!([]), "{resolved}");
    assert_eq!(check_state(&resolved), "passed");

    let review = json!({ "event": "REQUEST_CHANGES", "body": "Exercise the fallback deployment path." });
    let (status, body) = reviewer.send(Method::POST, &reviews, Some(review)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let reviewed_again = watch.changed(&resolved).await;
    assert!(reviewed_again["reviews"].as_array().unwrap().iter().any(|review| review["state"] == "changes_requested"), "{reviewed_again}");
    assert_eq!(check_state(&reviewed_again), "passed");

    let next = git.commit_file(&sample.dir, "landing.txt", "strong legs\n");
    git.ok(&sample.dir, &["push", "--quiet", &remote, "landing"]);
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut advanced = reviewed_again;
    while advanced["head"] != next {
        assert!(Instant::now() < deadline, "the push did not reach the pull request: {advanced}");
        advanced = watch.snapshot(Some(&advanced), 1).await;
    }
    assert_eq!(advanced["changed"], true, "{advanced}");
    assert_eq!(advanced["head"], next, "{advanced}");
    assert_eq!(check_state(&advanced), "expected", "the previous head's green check cannot carry forward");
    let (status, body) = owner.send(Method::PATCH, &watch.path, Some(json!({ "state": "closed" }))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let closed = watch.changed(&advanced).await;
    assert_eq!(closed["state"], "closed", "{closed}");
}
