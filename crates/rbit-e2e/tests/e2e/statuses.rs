//! Commit statuses and required checks end to end (milestone 3, S), over real HTTP and
//! the real `git` CLI: the owner protects `main` with a required `ci` check and a `status`
//! webhook; a pull request is blocked until an external CI (a token-authenticated API
//! client) reports `success` for `ci` on its head; then it merges. Along the way: the
//! combined status, scopes and validation of the status API, the checks on the pull
//! request and commit pages, the mark in the commit list, and 404 for hidden repositories.
//!
//! Milestone 3 workstream S (commit statuses and required checks) owns this file.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{extract::State, http::HeaderMap, routing::post};
use rbit_e2e::{Client, Git, StatusCode, TestServer};
use serde_json::{Value, json};

/// Fetches `path` until its body contains `needle`; returns the body.
async fn page_with(client: &Client, path: &str, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let response = client.get(path).await;
        if response.contains(needle) {
            return response.body;
        }
        assert!(Instant::now() < deadline, "{path} never showed {needle:?}:\n{}", response.body);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// ------------------------------------------------------------------ a webhook receiver

type Inbox = Arc<Mutex<Vec<(String, Value)>>>;

async fn start_receiver() -> (String, Inbox) {
    async fn receive(State(inbox): State<Inbox>, headers: HeaderMap, body: String) -> StatusCode {
        let event = headers.get("x-github-event").and_then(|value| value.to_str().ok()).unwrap_or_default().to_owned();
        inbox.lock().expect("inbox").push((event, serde_json::from_str(&body).unwrap_or(Value::Null)));
        StatusCode::NO_CONTENT
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind the receiver");
    let url = format!("http://{}/hook", listener.local_addr().expect("receiver address"));
    let inbox = Inbox::default();
    let app = axum::Router::new().route("/hook", post(receive)).with_state(Arc::clone(&inbox));
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve the receiver") });
    (url, inbox)
}

/// Waits for a delivery of `event` whose body satisfies `matches`; returns its body.
async fn delivery(inbox: &Inbox, event: &str, matches: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some((_, body)) = inbox.lock().expect("inbox").iter().find(|(name, body)| name == event && matches(body)) {
            return body.clone();
        }
        assert!(Instant::now() < deadline, "no matching {event} delivery: {:?}", inbox.lock().expect("inbox"));
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ------------------------------------------------------------------ an API client (the CI)

struct Api {
    http: reqwest::Client,
    base: String,
    token: Option<String>,
}

impl Api {
    fn new(server: &TestServer, token: Option<&str>) -> Self {
        let http = reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).build().expect("API client");
        Self { http, base: server.url("/api/v1"), token: token.map(str::to_owned) }
    }

    async fn send(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut request = self.http.request(method, format!("{}{path}", self.base));
        if let Some(token) = &self.token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        if let Some(body) = body {
            request = request.header("content-type", "application/json").body(body.to_string());
        }
        let response = request.send().await.expect("the API answers");
        let status = response.status();
        let text = response.text().await.expect("read the body");
        (status, if text.is_empty() { Value::Null } else { serde_json::from_str(&text).expect("API bodies are JSON") })
    }

    async fn get(&self, path: &str) -> (StatusCode, Value) {
        self.send(reqwest::Method::GET, path, None).await
    }

    async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.send(reqwest::Method::POST, path, Some(body)).await
    }
}

// ------------------------------------------------------------------ the scenario

#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one scenario, told in order")]
async fn a_required_check_blocks_the_merge_until_ci_reports_success() {
    let server = TestServer::start_with(|config| config.webhooks.allow_private_addresses = true);
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    let git_token = alice.create_token("git", "write").await;
    let ci_token = alice.create_token("ci", "write").await;
    let read_token = alice.create_token("reader", "read").await;
    let bobs_token = bob.create_token("bob", "write").await;

    alice.create_repo("alice", "demo", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", &git_token));
    let (hook_url, inbox) = start_receiver().await;
    alice
        .post("/alice/demo/settings/hooks", &[("url", &hook_url), ("event_status", "on"), ("active", "on")])
        .await
        .expect_status(StatusCode::SEE_OTHER);

    // main needs pull requests and a passing `ci` check.
    alice
        .post(
            "/alice/demo/settings/branches",
            &[("pattern", "main"), ("require_pull_request", "on"), ("require_status_checks", "on"), ("contexts", "ci\n")],
        )
        .await
        .expect_status(StatusCode::SEE_OTHER);
    alice.get("/alice/demo/settings/branches").await.expect_contains(&[">main<", ">Status checks<"]);
    let rules = Api::new(&server, Some(&alice.create_token("admin", "admin").await)).get("/repos/alice/demo/branch_protections").await;
    assert_eq!(rules.1[0]["required_status_checks"], json!(["ci"]), "{}", rules.1);

    // A topic branch and its pull request.
    let work = git.clone(&server.git_url_as("alice", "demo", "alice", &git_token));
    git.ok(&work, &["checkout", "--quiet", "-b", "topic"]);
    let head = git.commit_file(&work, "topic.txt", "a topic\n");
    git.ok(&work, &["push", "--quiet", "origin", "topic"]);
    alice
        .post("/alice/demo/pulls", &[("base", "main"), ("head", "topic"), ("title", "Add a topic"), ("body", "")])
        .await
        .expect_redirect("/alice/demo/pull/1");

    // Blocked: nothing reported yet, and no merge form for the owner either.
    let pull = page_with(&alice, "/alice/demo/pull/1", "No conflicts with the base branch").await;
    assert!(pull.contains("Waiting for 1 of 1 required checks") && pull.contains("Expected"), "{pull}");
    assert!(!pull.contains("name=\"expected_head\""), "no merge form while blocked");
    let merge = [("strategy", "merge"), ("expected_head", head.as_str()), ("title", ""), ("message", "")];
    alice.post("/alice/demo/pull/1/merge", &merge).await.expect_status(StatusCode::BAD_REQUEST);

    // The status API: scopes, access and validation.
    let ci = Api::new(&server, Some(&ci_token));
    let path = format!("/repos/alice/demo/statuses/{head}");
    let pending = json!({ "state": "pending", "context": "ci", "description": "Build started" });
    assert_eq!(Api::new(&server, None).post(&path, pending.clone()).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(Api::new(&server, Some(&read_token)).post(&path, pending.clone()).await.0, StatusCode::FORBIDDEN, "read scope");
    assert_eq!(Api::new(&server, Some(&bobs_token)).post(&path, pending.clone()).await.0, StatusCode::FORBIDDEN, "bob only reads");
    let (status, body) = ci.post(&path, json!({ "state": "done" })).await;
    assert_eq!((status, body["errors"][0]["field"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("state")));
    let (status, body) = ci.post(&path, json!({ "state": "success", "target_url": "javascript:alert(1)" })).await;
    assert_eq!((status, body["errors"][0]["field"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("target_url")));
    let (status, _) = ci.post(&format!("/repos/alice/demo/statuses/{}", "0".repeat(40)), pending.clone()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "an unknown commit");
    let (status, created) = ci.post(&path, pending).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!((created["state"].as_str(), created["context"].as_str()), (Some("pending"), Some("ci")));
    assert_eq!(created["creator"]["login"], "alice");

    let (status, combined) = Api::new(&server, None).get("/repos/alice/demo/commits/topic/status").await;
    assert_eq!(status, StatusCode::OK, "anyone may read a public repository's statuses");
    assert_eq!(
        (combined["state"].as_str(), combined["sha"].as_str(), combined["total_count"].as_u64()),
        (Some("pending"), Some(head.as_str()), Some(1))
    );
    let pull = page_with(&alice, "/alice/demo/pull/1", "Build started").await;
    assert!(pull.contains("Waiting for 1 of 1 required checks"), "pending is not success");

    // CI reports success: the newest report wins and the merge goes through.
    let success = json!({ "state": "success", "context": "ci", "description": "All green", "target_url": "https://ci.example.com/b/1" });
    assert_eq!(ci.post(&path, success).await.0, StatusCode::CREATED);
    let (_, history) = ci.get("/repos/alice/demo/commits/topic/statuses").await;
    let reported: Vec<_> = history.as_array().expect("a list").iter().map(|status| status["state"].as_str().unwrap_or_default()).collect();
    assert_eq!(reported, ["success", "pending"], "newest first");
    let (_, combined) = ci.get(&format!("/repos/alice/demo/commits/{head}/status")).await;
    assert_eq!(combined["state"], "success");
    let (status, commit) = ci.get("/repos/alice/demo/commits/topic").await;
    assert_eq!((status, commit["sha"].as_str()), (StatusCode::OK, Some(head.as_str())), "the commit endpoint still answers");

    let hook = delivery(&inbox, "status", |body| body["state"] == "success").await;
    assert_eq!((hook["sha"].as_str(), hook["context"].as_str()), (Some(head.as_str()), Some("ci")));
    assert_eq!((hook["name"].as_str(), hook["sender"]["login"].as_str()), (Some("alice/demo"), Some("alice")));
    assert_eq!(hook["branches"][0]["name"], "topic");
    assert_eq!(hook["target_url"], "https://ci.example.com/b/1");

    let pull = page_with(&alice, "/alice/demo/pull/1", "1 required check passed").await;
    assert!(pull.contains("All green") && pull.contains("href=\"https://ci.example.com/b/1\""), "{pull}");
    assert!(pull.contains("name=\"expected_head\""), "the merge form is offered now");
    assert!(!pull.contains("style="), "no inline styles");
    alice.post("/alice/demo/pull/1/merge", &merge).await.expect_redirect("/alice/demo/pull/1");
    alice.get("/alice/demo/pull/1").await.expect_contains(&["Pull request merged"]);
    let merged = git.clone(&server.git_url("alice", "demo"));
    assert_eq!(git.ok(&merged, &["log", "-1", "--format=%P"]).trim(), format!("{} {head}", sample.head), "a merge commit");

    // The commit page lists the checks; the commit list shows the combined mark.
    alice.get(&format!("/alice/demo/commit/{head}")).await.expect_contains(&["id=\"checks\"", ">ci<", "All green", "1 successful check"]);
    let commits = bob.get("/alice/demo/commits/topic").await;
    commits.expect_contains(&[&format!("href=\"/alice/demo/commit/{head}#checks\""), "Success: 1 successful check"]);

    // A private repository's statuses do not exist for outsiders.
    alice.create_repo("alice", "vault", "private").await;
    let vault = git.sample();
    git.push_sample(&vault, &server.git_url_as("alice", "vault", "alice", &git_token));
    let path = format!("/repos/alice/vault/statuses/{}", vault.head);
    assert_eq!(ci.post(&path, json!({ "state": "failure" })).await.0, StatusCode::CREATED);
    let outsider = Api::new(&server, Some(&bobs_token));
    for path in ["/repos/alice/vault/commits/main/status", "/repos/alice/vault/commits/main/statuses"] {
        assert_eq!(outsider.get(path).await.0, StatusCode::NOT_FOUND, "{path}");
    }
    assert_eq!(outsider.post(&path, json!({ "state": "success" })).await.0, StatusCode::NOT_FOUND);
    assert_eq!(bob.get(&format!("/alice/vault/commit/{}", vault.head)).await.status, StatusCode::NOT_FOUND);
    let (_, combined) = ci.get("/repos/alice/vault/commits/main/status").await;
    assert_eq!((combined["state"].as_str(), combined["statuses"][0]["context"].as_str()), (Some("failure"), Some("default")));
}
