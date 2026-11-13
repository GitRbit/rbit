//! Landing safely, end to end (agent plan steps 4.1 to 4.3 and 5.4): an issue taken once,
//! a pull request ensured twice, its readiness read with the merge's own rules, and a merge
//! queue that tests the merge of the current base and lands exactly that commit.
//!
//! No runner: `main` requires a `ci/test` status, which the test reports itself on the
//! queue's candidate, as a CI system would.
//!
//! The integration workstream (I) owns this file.

use std::time::{Duration, Instant};

use rbit_e2e::{Git, PASSWORD, TestServer};
use rbit_services::pulls::queue::QUEUE_SWEEP;
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

pub(crate) struct Api {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl Api {
    pub(crate) fn new(server: &TestServer, token: String) -> Self {
        Self { http: reqwest::Client::builder().no_proxy().build().unwrap(), base: server.base_url().to_owned(), token }
    }

    pub(crate) async fn send(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut request = self.http.request(method, format!("{}/api/v1/repos/alice/rocket/{path}", self.base)).bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.header("Content-Type", "application/json").body(body.to_string());
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let text = response.text().await.unwrap();
        (status, if text.is_empty() { Value::Null } else { serde_json::from_str(&text).unwrap() })
    }

    /// The answers to four copies of one `pulls/ensure` sent at once: success and number.
    async fn ensure_copies(&self, body: Value) -> std::collections::BTreeSet<(bool, Option<u64>)> {
        let copy = || self.send(Method::POST, "pulls/ensure", Some(body.clone()));
        let (one, two, three, four) = tokio::join!(copy(), copy(), copy(), copy());
        [one, two, three, four].iter().map(|(status, body)| (status.is_success(), body["number"].as_u64())).collect()
    }

    /// The queue entry once `ready` says so, allowing the documented fallback sweep.
    pub(crate) async fn queue_until(&self, number: u64, ready: impl Fn(&Value) -> bool) -> Value {
        // A push may be observed before Actions records its decision, with no later status
        // event when no workflow starts. Allow two sweeps plus scheduling headroom.
        let deadline = Instant::now() + QUEUE_SWEEP * 2 + Duration::from_secs(20);
        loop {
            let (status, entry) = self.send(Method::GET, &format!("pulls/{number}/queue"), None).await;
            assert_eq!(status, StatusCode::OK, "reading queue entry #{number}: {entry}");
            if ready(&entry) {
                return entry;
            }
            assert!(Instant::now() < deadline, "the queue entry of #{number} never got there: {status}: {entry}");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

#[tokio::test]
async fn a_queued_pull_request_lands_exactly_the_commit_ci_passed() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "rocket", "public").await;
    let api = Api::new(&server, alice.create_token("agent", "admin").await);
    let bobs = Api::new(&server, bob.create_token("agent", "write").await);
    let (status, _) = api.send(Method::PUT, "collaborators/bob", Some(json!({ "permission": "push" }))).await;
    assert_eq!(status, StatusCode::CREATED);

    // 5.4: an issue is taken once; the second taker hears who holds it.
    let (_, issue) = api.send(Method::POST, "issues", Some(json!({ "title": "Make rockets land" }))).await;
    let number = issue["number"].as_u64().unwrap();
    let (status, taken) = api.send(Method::POST, &format!("issues/{number}/take"), Some(json!({}))).await;
    assert_eq!((status, taken["assignees"][0]["login"].as_str()), (StatusCode::OK, Some("alice")), "{taken}");
    assert_eq!(
        api.send(Method::POST, &format!("issues/{number}/take"), Some(json!({}))).await.0,
        StatusCode::OK,
        "taken again by its holder"
    );
    let (status, refused) = bobs.send(Method::POST, &format!("issues/{number}/take"), Some(json!({}))).await;
    assert_eq!((status, refused["code"].as_str()), (StatusCode::CONFLICT, Some("conflict")), "{refused}");
    assert!(refused["message"].as_str().unwrap().contains("alice"), "{refused}");
    let (_, found) = api.send(Method::GET, "agent/issues?q=rockets+land", None).await;
    assert_eq!(found["items"][0]["number"].as_u64(), Some(number), "{found}");
    assert_eq!(api.send(Method::GET, "agent/issues?q=%25+--", None).await.0, StatusCode::UNPROCESSABLE_ENTITY, "no word, no search");

    // main needs pull requests and a passing `ci/test`.
    let git = Git::new();
    let sample = git.sample();
    let url = server.git_url_as("alice", "rocket", "alice", PASSWORD);
    git.commit_file(&sample.dir, "rocket.txt", "v1\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "main"]);
    let protection = json!({ "pattern": "main", "require_pull_request": true, "required_approvals": 0, "require_status_checks": true,
        "required_status_checks": ["ci/test"] });
    assert_eq!(api.send(Method::POST, "branch_protections", Some(protection)).await.0, StatusCode::CREATED);
    git.ok(&sample.dir, &["checkout", "--quiet", "-b", "landing"]);
    let head = git.commit_file(&sample.dir, "landing.txt", "legs\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "landing"]);

    // 4.3: ensuring twice gives one pull request; a request id replays; reused, it conflicts.
    let ensure = json!({ "head": "landing", "base": "main", "title": "Landing legs", "request_id": "r-1" });
    let (status, first) = api.send(Method::POST, "pulls/ensure", Some(ensure.clone())).await;
    assert_eq!(
        (status, first["outcome"].as_str(), first["pull"]["draft"].as_bool()),
        (StatusCode::CREATED, Some("created"), Some(true)),
        "{first}"
    );
    let pull = first["number"].as_u64().unwrap();
    let (status, again) = api.send(Method::POST, "pulls/ensure", Some(ensure)).await;
    assert_eq!((status, again["outcome"].as_str(), again["number"].as_u64()), (StatusCode::OK, Some("replayed"), Some(pull)));
    let (_, existing) = api.send(Method::POST, "pulls/ensure", Some(json!({ "head": "landing", "base": "main" }))).await;
    assert_eq!((existing["outcome"].as_str(), existing["number"].as_u64()), (Some("existing"), Some(pull)));
    let other_inputs = json!({ "head": "landing", "base": "main", "title": "Other", "request_id": "r-1" });
    assert_eq!(api.send(Method::POST, "pulls/ensure", Some(other_inputs)).await.0, StatusCode::CONFLICT);
    // Racing copies of one request (a retry sent while the first runs) get one pull request.
    git.ok(&sample.dir, &["checkout", "--quiet", "-b", "racing", "main"]);
    git.commit_file(&sample.dir, "race.txt", "go\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "racing"]);
    let answers = api.ensure_copies(json!({ "head": "racing", "base": "main", "request_id": "r-race" })).await;
    assert_eq!(answers.len(), 1, "every copy answers the same pull request: {answers:?}");
    git.ok(&sample.dir, &["checkout", "--quiet", "landing"]);

    // 4.2: a draft is not ready; marked ready for its head, it waits only on `ci/test`.
    let (_, readiness) = api.send(Method::GET, &format!("pulls/{pull}/readiness"), None).await;
    assert_eq!(readiness["ready"], false, "{readiness}");
    let stale = json!({ "head": "0".repeat(40) });
    let (status, moved) = api.send(Method::POST, &format!("pulls/{pull}/ready"), Some(stale)).await;
    assert_eq!((status, moved["code"].as_str()), (StatusCode::CONFLICT, Some("head_changed")));
    let (_, readiness) = api.send(Method::POST, &format!("pulls/{pull}/ready"), Some(json!({ "head": head }))).await;
    assert_eq!(readiness["draft"], false, "{readiness}");
    let check = readiness["checks"].as_array().unwrap().iter().find(|check| check["context"] == "ci/test").cloned().unwrap();
    assert_eq!((check["required"].as_bool(), check["state"].as_str()), (Some(true), Some("expected")), "{readiness}");

    // 4.1: queued, its merge with the current main is built and waits for `ci/test`.
    let (status, place) = api.send(Method::POST, &format!("pulls/{pull}/queue"), Some(json!({ "head": head }))).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{place}");
    let testing = api.queue_until(pull, |entry| entry["state"] == "testing" && entry["candidate"].is_string()).await;
    let first_candidate = testing["candidate"].as_str().unwrap().to_owned();
    assert_eq!(testing["position"], 1);

    // Nobody but the server writes candidates.
    let refused = git.fails(&sample.dir, &["push", &url, &format!("+{head}:refs/merge-queue/main/pr-{pull}")]);
    assert!(refused.contains("managed by rbit"), "{refused}");

    // main moves before CI reports: the old candidate never lands; a new one is built.
    git.ok(&sample.dir, &["checkout", "--quiet", "main"]);
    git.ok(&sample.dir, &["checkout", "--quiet", "-b", "hotfix"]);
    let hotfix_head = git.commit_file(&sample.dir, "fuel.txt", "more\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "hotfix"]);
    let status_path = |sha: &str| format!("statuses/{sha}");
    let success = json!({ "state": "success", "context": "ci/test" });
    api.send(Method::POST, &status_path(&hotfix_head), Some(success.clone())).await;
    let (_, hotfix) = api.send(Method::POST, "pulls", Some(json!({ "title": "Fuel", "head": "hotfix", "base": "main" }))).await;
    let merge = json!({ "sha": hotfix_head, "merge_method": "merge" });
    let (status, merged) = api.send(Method::PUT, &format!("pulls/{}/merge", hotfix["number"]), Some(merge)).await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    api.send(Method::POST, &status_path(&first_candidate), Some(success.clone())).await;
    let rebuilt = api.queue_until(pull, |entry| entry["candidate"].as_str().is_some_and(|sha| sha != first_candidate)).await;
    assert_eq!(rebuilt["state"], "testing", "a candidate on the old main must not land: {rebuilt}");
    let (_, main) = api.send(Method::GET, "commits/main", None).await;
    assert_ne!(main["sha"].as_str(), Some(first_candidate.as_str()));

    // CI passes on the new candidate: exactly that commit becomes main.
    let candidate = rebuilt["candidate"].as_str().unwrap().to_owned();
    api.send(Method::POST, &status_path(&candidate), Some(success)).await;
    let landed = api.queue_until(pull, |entry| entry["state"] != "testing").await;
    assert_eq!(landed["state"], "landed", "{landed}");
    let (_, main) = api.send(Method::GET, "commits/main", None).await;
    assert_eq!(main["sha"].as_str(), Some(candidate.as_str()), "main is the tested commit");
    let (_, landed_pull) = api.send(Method::GET, &format!("pulls/{pull}"), None).await;
    assert_eq!(landed_pull["merged"], true, "{landed_pull}");

    // 4.5: the commit queries know where it went.
    let (_, pulls) = api.send(Method::GET, &format!("agent/commits/{candidate}/pulls"), None).await;
    assert_eq!((pulls["pulls"][0]["number"].as_u64(), pulls["pulls"][0]["role"].as_str()), (Some(pull), Some("merge_commit")), "{pulls}");
    let (_, contains) = api.send(Method::GET, &format!("agent/contains?commit={head}"), None).await;
    assert_eq!(contains["contains"], true, "{contains}");
    let (_, compared) = api.send(Method::GET, &format!("agent/compare?base=main&head={head}"), None).await;
    assert_eq!((compared["ahead"].as_u64(), compared["merge_base"].as_str()), (Some(0), Some(head.as_str())), "{compared}");
}
