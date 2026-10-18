//! Issues, labels, references and notifications, end to end.
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! Notifications, commit references and webhook deliveries are produced by background
//! subscribers, so the tests poll for them (bounded) instead of expecting them at once.

use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{extract::State, http::HeaderMap, routing::post};
use hmac::{Hmac, KeyInit, Mac};
use rbit_e2e::{Client, Git, PASSWORD, Sample, StatusCode, TestServer};
use sha2::Sha256;

const OPEN_BADGE: &str = "class=\"state-badge state-badge-open\"";
const CLOSED_BADGE: &str = "class=\"state-badge state-badge-closed\"";

/// Polls `check` until it holds, for at most 20 seconds.
async fn eventually<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = Instant::now() + Duration::from_secs(20);
    while !check().await {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Whether the header bell shows the unread dot for `client`.
async fn has_unread(client: &Client) -> bool {
    client.get("/").await.contains("<span class=\"notification-dot\"")
}

/// A JSON API call with a bearer token.
async fn api(
    server: &TestServer,
    method: reqwest::Method,
    path: &str,
    token: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, HeaderMap, serde_json::Value) {
    let http = reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).build().expect("API client");
    let url = if path.starts_with("http") { path.to_owned() } else { server.url(&format!("/api/v1{path}")) };
    let mut request = http.request(method, url).header("authorization", format!("Bearer {token}"));
    if let Some(body) = body {
        request = request.header("content-type", "application/json").body(body.to_string());
    }
    let response = request.send().await.expect("the API answers");
    let (status, headers) = (response.status(), response.headers().clone());
    let text = response.text().await.expect("read the body");
    let json = if text.is_empty() { serde_json::Value::Null } else { serde_json::from_str(&text).expect("JSON") };
    (status, headers, json)
}

// ------------------------------------------------------------------ a webhook receiver

#[derive(Debug, Clone)]
struct Received {
    headers: HeaderMap,
    body: String,
}

type Inbox = Arc<Mutex<Vec<Received>>>;

async fn start_receiver() -> (String, Inbox) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind the receiver");
    let url = format!("http://{}/hook", listener.local_addr().expect("receiver address"));
    let inbox = Inbox::default();
    let app = axum::Router::new().route("/hook", post(receive)).with_state(Arc::clone(&inbox));
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve the receiver") });
    (url, inbox)
}

async fn receive(State(inbox): State<Inbox>, headers: HeaderMap, body: String) -> StatusCode {
    inbox.lock().expect("inbox").push(Received { headers, body });
    StatusCode::NO_CONTENT
}

fn signed_with(received: &Received, secret: &str) -> bool {
    let Some(hex) = received.headers.get("x-hub-signature-256").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("sha256="))
    else {
        return false;
    };
    let Some(signature) =
        (0..hex.len()).step_by(2).map(|i| hex.get(i..i + 2).and_then(|p| u8::from_str_radix(p, 16).ok())).collect::<Option<Vec<u8>>>()
    else {
        return false;
    };
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).expect("any key length");
    mac.update(received.body.as_bytes());
    mac.verify_slice(&signature).is_ok()
}

// ------------------------------------------------------------------ tests

#[tokio::test]
async fn outsiders_get_404_for_a_private_repositorys_issues() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "vault", "private").await;
    alice.post("/alice/vault/issues", &[("title", "Secret plans")]).await.expect_redirect("/alice/vault/issues/1");
    alice.get("/alice/vault/issues").await.expect_status(StatusCode::OK).expect_contains(&["Secret plans"]);
    let token = bob.create_token("api", "admin").await;
    for client in [&bob, &server.client()] {
        for path in ["/alice/vault/issues", "/alice/vault/issues/1", "/alice/vault/labels", "/alice/vault/milestones"] {
            client.get(path).await.expect_status(StatusCode::NOT_FOUND);
        }
    }
    bob.get("/alice/vault/issues/new").await.expect_status(StatusCode::NOT_FOUND);
    server.client().get("/alice/vault/issues/new").await.expect_status(StatusCode::SEE_OTHER);
    bob.post("/alice/vault/issues", &[("title", "Let me in")]).await.expect_status(StatusCode::NOT_FOUND);
    bob.post("/alice/vault/issues/1/labels", &[("label", "1")]).await.expect_status(StatusCode::NOT_FOUND);
    for path in ["/repos/alice/vault/issues", "/repos/alice/vault/issues/1", "/repos/alice/vault/labels"] {
        let (status, _, _) = api(&server, reqwest::Method::GET, path, &token, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
    alice.get("/alice/vault/issues/abc").await.expect_status(StatusCode::NOT_FOUND);
    alice.get("/alice/vault/issues/7").await.expect_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn triage_notifications_references_and_webhooks() {
    let server = TestServer::start_with(|config| config.webhooks.allow_private_addresses = true);
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    let carol = server.sign_up("carol").await;
    alice.create_repo("alice", "demo", "private").await;
    let collaborators = "/alice/demo/settings/collaborators";
    alice.post(collaborators, &[("username", "bob"), ("permission", "write")]).await.expect_redirect(collaborators);
    let token = alice.create_token("e2e", "admin").await;

    // A signed `issues` webhook.
    let (receiver, inbox) = start_receiver().await;
    let hook = serde_json::json!({ "config": { "url": receiver, "secret": "s3cret" }, "events": ["issues"], "active": true });
    let (status, _, created) = api(&server, reqwest::Method::POST, "/repos/alice/demo/hooks", &token, Some(hook)).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");

    // Labels, then an issue labelled and assigned to bob that mentions carol (an outsider).
    alice
        .post("/alice/demo/labels", &[("name", "bug"), ("color", "#d73a4a"), ("description", "")])
        .await
        .expect_redirect("/alice/demo/labels");
    alice.get("/alice/demo/labels").await.expect_contains(&["bug", "fill=\"#d73a4a\""]);
    let (_, _, labels) = api(&server, reqwest::Method::GET, "/repos/alice/demo/labels", &token, None).await;
    let label_id = labels[0]["id"].as_i64().expect("label id").to_string();
    let (_, _, assignable) = api(&server, reqwest::Method::GET, "/repos/alice/demo/assignees", &token, None).await;
    let bob_id =
        assignable.as_array().unwrap().iter().find(|a| a["login"] == "bob").expect("bob is assignable")["id"].as_i64().unwrap().to_string();
    alice.get("/alice/demo/issues/new").await.expect_status(StatusCode::OK).expect_contains(&["Labels", "Assignees", "bob"]);
    let opened = alice
        .post(
            "/alice/demo/issues",
            &[("title", "Crash on start"), ("body", "Boom. cc @carol and @bob"), ("label", &label_id), ("assignee", &bob_id)],
        )
        .await;
    opened.expect_redirect("/alice/demo/issues/1");

    let page = alice.get("/alice/demo/issues/1").await;
    page.expect_status(StatusCode::OK).expect_contains(&["Crash on start", OPEN_BADGE, "Edit labels", "bug", "bob"]);
    assert!(!page.contains("style="), "no inline styles");
    alice.get("/alice/demo/issues").await.expect_contains(&["Crash on start", "1</span> Open"]);
    alice.get("/alice/demo/issues?q=is%3Aopen+label%3Abug+crash").await.expect_contains(&["Crash on start"]);
    alice.get("/alice/demo/issues?q=label%3Anope").await.expect_contains(&["No issues match"]);

    // Bob (assigned) sees the unread dot; carol (mentioned, no access) does not.
    eventually("bob's notification", || has_unread(&bob)).await;
    bob.get("/notifications").await.expect_contains(&["Crash on start", "Assigned", "alice/demo"]);
    assert!(!has_unread(&carol).await, "a mentioned outsider is never notified");
    carol.get("/notifications").await.expect_contains(&["All caught up"]);
    assert!(!has_unread(&alice).await, "the actor is never notified");
    // Visiting the issue marks it read.
    bob.get("/alice/demo/issues/1").await.expect_status(StatusCode::OK);
    assert!(!has_unread(&bob).await);

    // Bob comments and assigns; alice (the author) hears about it.
    bob.post("/alice/demo/issues/1/comments", &[("body", "On it")]).await.expect_status(StatusCode::SEE_OTHER);
    eventually("alice's notification", || has_unread(&alice)).await;

    // The opened webhook arrived, signed, with GitHub's fields.
    eventually("the issues webhook", || {
        let inbox = Arc::clone(&inbox);
        async move { inbox.lock().unwrap().iter().any(|r| r.headers.get("x-github-event").is_some_and(|v| v == "issues")) }
    })
    .await;
    let deliveries = inbox.lock().unwrap().clone();
    let opened = deliveries.iter().find(|r| r.headers.get("x-github-event").is_some_and(|v| v == "issues")).unwrap();
    assert!(signed_with(opened, "s3cret"), "the delivery is signed");
    let body: serde_json::Value = serde_json::from_str(&opened.body).unwrap();
    assert_eq!((body["action"].as_str(), body["issue"]["number"].as_u64()), (Some("opened"), Some(1)));
    assert_eq!(body["issue"]["labels"][0]["name"], "bug");
    assert!(!opened.body.contains("@example.test"), "no email addresses");

    // `Fixes #1` on a feature branch only references; on main it closes.
    let clone = git.clone(&server.git_url_as("alice", "demo", "alice", &token));
    git.commit_file(&clone, "README.md", "# Demo\n");
    git.ok(&clone, &["push", "--quiet", "origin", "HEAD:refs/heads/main"]);
    git.ok(&clone, &["checkout", "--quiet", "-b", "fix"]);
    std::fs::write(clone.join("fix.txt"), "fixed\n").unwrap();
    git.ok(&clone, &["add", "--all"]);
    git.ok(&clone, &["commit", "--quiet", "-m", "Stop the crash", "-m", "Fixes #1"]);
    let fix = git.ok(&clone, &["rev-parse", "HEAD"]).trim().to_owned();
    git.ok(&clone, &["push", "--quiet", "origin", "fix"]);
    let short = &fix[..7];
    eventually("the commit reference", || {
        let alice = &alice;
        async move { alice.get("/alice/demo/issues/1").await.contains(short) }
    })
    .await;
    alice.get("/alice/demo/issues/1").await.expect_contains(&[OPEN_BADGE]);
    git.ok(&clone, &["push", "--quiet", "origin", "fix:main"]);
    eventually("the issue to close", || {
        let alice = &alice;
        async move { alice.get("/alice/demo/issues/1").await.contains(CLOSED_BADGE) }
    })
    .await;
    alice.get("/alice/demo/issues?q=is%3Aclosed").await.expect_contains(&["Crash on start"]);

    // The API pages through issues with Link headers.
    for title in ["Second", "Third"] {
        let (status, _, _) =
            api(&server, reqwest::Method::POST, "/repos/alice/demo/issues", &token, Some(serde_json::json!({ "title": title }))).await;
        assert_eq!(status, StatusCode::CREATED);
    }
    let (status, headers, first) = api(&server, reqwest::Method::GET, "/repos/alice/demo/issues?state=all&per_page=2", &token, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first.as_array().unwrap().len(), 2);
    assert_eq!(headers["x-total-count"], "3");
    let link = headers["link"].to_str().unwrap();
    let next = link
        .split(',')
        .find(|part| part.contains("rel=\"next\""))
        .and_then(|part| part.split(['<', '>']).nth(1))
        .expect("a next link")
        .to_owned();
    let (_, _, second) = api(&server, reqwest::Method::GET, &next, &token, None).await;
    assert_eq!(second[0]["title"], "Crash on start");
    assert_eq!(second[0]["state"], "closed");
}

#[tokio::test]
async fn milestones_and_labels_pages_for_writers_only() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "demo", "public").await;
    alice.post("/alice/demo/labels/defaults", &[]).await.expect_redirect("/alice/demo/labels");
    alice.get("/alice/demo/labels").await.expect_contains(&["good first issue", "9 labels"]);
    bob.get("/alice/demo/labels").await.expect_status(StatusCode::OK).expect_contains(&["good first issue"]);
    bob.post("/alice/demo/labels", &[("name", "mine"), ("color", "#000000")]).await.expect_status(StatusCode::FORBIDDEN);
    alice.post("/alice/demo/labels", &[("name", "BUG"), ("color", "#000000")]).await.expect_contains(&["already taken"]);

    let created =
        alice.post("/alice/demo/milestones", &[("title", "v1.0"), ("due_on", "2030-01-15"), ("description", "First **release**")]).await;
    created.expect_status(StatusCode::SEE_OTHER);
    let milestone = created.location().unwrap().to_owned();
    alice.get(&milestone).await.expect_contains(&["v1.0", "Jan 15, 2030", "<strong>release</strong>", "width=\"0\""]);
    bob.get("/alice/demo/milestones/new").await.expect_status(StatusCode::NOT_FOUND);
    bob.post("/alice/demo/milestones", &[("title", "x")]).await.expect_status(StatusCode::NOT_FOUND);
    alice.post("/alice/demo/milestones", &[("title", "v2"), ("due_on", "2030-02-30")]).await.expect_contains(&["YYYY-MM-DD"]);

    // An issue in the milestone, closed: the bar is full.
    bob.post("/alice/demo/issues", &[("title", "Ship it")]).await.expect_redirect("/alice/demo/issues/1");
    let id = milestone.rsplit('/').next().unwrap().to_owned();
    alice.post("/alice/demo/issues/1/milestone", &[("milestone", &id)]).await.expect_status(StatusCode::SEE_OTHER);
    bob.post("/alice/demo/issues/1/milestone", &[("milestone", "")]).await.expect_status(StatusCode::FORBIDDEN);
    alice.post("/alice/demo/issues/1/state", &[("state", "close_completed")]).await.expect_status(StatusCode::SEE_OTHER);
    alice.get("/alice/demo/milestones").await.expect_contains(&["v1.0", "width=\"100\""]);
    alice.get(&format!("{milestone}?state=closed")).await.expect_contains(&["Ship it"]);
    alice.get("/alice/demo/issues/1").await.expect_contains(&["added this to the", "v1.0"]);

    // Deleting an issue is for administrators; the list forgets it.
    bob.post("/alice/demo/issues/1/delete", &[]).await.expect_status(StatusCode::FORBIDDEN);
    alice.post("/alice/demo/issues/1/delete", &[]).await.expect_redirect("/alice/demo/issues");
    alice.get("/alice/demo/issues/1").await.expect_status(StatusCode::NOT_FOUND);
}

/// Viewing a discussion marks its notification read, so a signed-in viewer's preload (a
/// hover, a link scrolled into view) is declined: it must not mark a discussion read that
/// was never opened, and its answer must not be stored, so the click reaches the server
/// and marks it read then.
#[tokio::test]
async fn preloading_a_discussion_never_marks_it_read() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "demo", "public").await;
    git.push_sample(&git.sample(), &server.git_url_as("alice", "demo", "alice", PASSWORD));
    alice.post("/alice/demo/issues", &[("title", "Crash on start"), ("body", "")]).await.expect_redirect("/alice/demo/issues/1");
    alice
        .post("/alice/demo/pulls", &[("base", "main"), ("head", Sample::BRANCH), ("title", "Feature"), ("body", "")])
        .await
        .expect_redirect("/alice/demo/pull/2");

    let preload = [("HX-Request", "true"), ("HX-Preloaded", "true")];
    for (number, path) in [(1, "/alice/demo/issues/1"), (2, "/alice/demo/pull/2")] {
        bob.post(&format!("/alice/demo/issues/{number}/comments"), &[("body", "Ping")]).await.expect_status(StatusCode::SEE_OTHER);
        eventually("alice's notification", || has_unread(&alice)).await;

        let preloaded = alice.get_with(path, &preload).await;
        preloaded.expect_status(StatusCode::NO_CONTENT);
        assert_eq!(preloaded.header("cache-control"), Some("no-store"), "the click must not be answered from the cache");
        assert!(has_unread(&alice).await, "a preload of {path} is not a visit");
        // Without read state to keep, anonymous preloads are served as usual.
        server.client().get_with(path, &preload).await.expect_status(StatusCode::OK);

        alice.get(path).await.expect_status(StatusCode::OK);
        assert!(!has_unread(&alice).await, "visiting {path} marks it read");
    }
}
