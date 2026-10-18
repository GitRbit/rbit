//! The whole milestone 2 workflow in one scenario, over real HTTP and the real `git` CLI:
//! an owner protects `main`, a collaborator files an issue, fixes it on a branch and opens
//! a pull request that says "Fixes #1"; protection refuses the shortcuts, the owner
//! reviews (a line comment and an approval) and squash-merges, and the merge closes the
//! issue. Along the way: cross-references on the issue's timeline, notifications for the
//! right people (never the actor) and signed webhooks for every event.
//!
//! The integrator owns this file.
//!
//! Notifications, references, mergeability and webhook deliveries are background work,
//! so the test polls for them (bounded). Both users sign up once (Argon2 is slow in debug
//! builds); `git` authenticates with personal access tokens for the same reason.

use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{extract::State, http::HeaderMap, routing::post};
use hmac::{Hmac, KeyInit, Mac};
use rbit_e2e::{Client, Git, StatusCode, TestServer};
use serde_json::Value;
use sha2::Sha256;

const SECRET: &str = "workflow-secret";

/// Polls `check` until it holds, for at most 30 seconds.
async fn eventually<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = Instant::now() + Duration::from_secs(30);
    while !check().await {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

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

/// Whether the header bell shows the unread dot for `client`.
async fn has_unread(client: &Client) -> bool {
    client.get("/").await.contains("<span class=\"notification-dot\"")
}

// ------------------------------------------------------------------ a webhook receiver

/// One delivery: its `X-GitHub-Event`, whether its HMAC verifies with [`SECRET`], the body.
#[derive(Debug, Clone)]
struct Delivery {
    event: String,
    signed: bool,
    body: Value,
}

type Inbox = Arc<Mutex<Vec<Delivery>>>;

async fn start_receiver() -> (String, Inbox) {
    async fn receive(State(inbox): State<Inbox>, headers: HeaderMap, body: String) -> StatusCode {
        let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok()).unwrap_or_default().to_owned();
        let delivery = Delivery {
            event: header("x-github-event"),
            signed: verifies(&header("x-hub-signature-256"), &body),
            body: serde_json::from_str(&body).unwrap_or(Value::Null),
        };
        inbox.lock().expect("inbox").push(delivery);
        StatusCode::NO_CONTENT
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind the receiver");
    let url = format!("http://{}/hook", listener.local_addr().expect("receiver address"));
    let inbox = Inbox::default();
    let app = axum::Router::new().route("/hook", post(receive)).with_state(Arc::clone(&inbox));
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve the receiver") });
    (url, inbox)
}

/// Checks a `sha256=<hex>` signature of `body` with an HMAC independent of the server's.
fn verifies(signature: &str, body: &str) -> bool {
    let Some(hex) = signature.strip_prefix("sha256=") else { return false };
    let Some(bytes) = (0..hex.len())
        .step_by(2)
        .map(|i| hex.get(i..i + 2).and_then(|pair| u8::from_str_radix(pair, 16).ok()))
        .collect::<Option<Vec<u8>>>()
    else {
        return false;
    };
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(SECRET.as_bytes()).expect("any key length");
    mac.update(body.as_bytes());
    mac.verify_slice(&bytes).is_ok()
}

/// Waits for a delivery of `event` whose body satisfies `matches`; returns its body.
async fn delivery(inbox: &Inbox, event: &str, matches: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(found) = inbox.lock().expect("inbox").iter().find(|d| d.event == event && matches(&d.body)) {
            assert!(found.signed, "the {event} delivery is signed with the hook's secret");
            return found.body.clone();
        }
        let seen: Vec<(String, Value)> = inbox.lock().expect("inbox").iter().map(|d| (d.event.clone(), d.body["action"].clone())).collect();
        assert!(Instant::now() < deadline, "no matching {event} delivery; received {seen:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ------------------------------------------------------------------ the scenario

#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one scenario, told in order")]
async fn an_issue_is_fixed_through_a_protected_reviewed_pull_request() {
    let server = TestServer::start_with(|config| config.webhooks.allow_private_addresses = true);
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    let alice_token = alice.create_token("git", "write").await;
    let bob_token = bob.create_token("git", "write").await;

    // Alice creates the repository, pushes, and adds a signed webhook for every event here.
    alice.create_repo("alice", "demo", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", &alice_token));
    let (hook_url, inbox) = start_receiver().await;
    alice
        .post(
            "/alice/demo/settings/hooks",
            &[
                ("url", &hook_url),
                ("secret", SECRET),
                ("event_push", "on"),
                ("event_issues", "on"),
                ("event_pull_request", "on"),
                ("event_pull_request_review", "on"),
                ("active", "on"),
            ],
        )
        .await
        .expect_status(StatusCode::SEE_OTHER);

    // She protects main: pull requests with one approval, no force pushes.
    let rule = alice
        .post(
            "/alice/demo/settings/branches",
            &[("pattern", "main"), ("require_pull_request", "on"), ("required_approvals", "1"), ("deny_force_push", "on")],
        )
        .await;
    rule.expect_status(StatusCode::SEE_OTHER);
    alice.get("/alice/demo/settings/branches").await.expect_contains(&[">main<", ">1 approval<", ">No force pushes<"]);

    // Bob becomes a collaborator (write) and files an issue.
    let collaborators = "/alice/demo/settings/collaborators";
    alice.post(collaborators, &[("username", "bob"), ("permission", "write")]).await.expect_redirect(collaborators);
    bob.post("/alice/demo/issues", &[("title", "Crash on start"), ("body", "It crashes when started.")])
        .await
        .expect_redirect("/alice/demo/issues/1");
    let opened = delivery(&inbox, "issues", |body| body["action"] == "opened").await;
    assert_eq!((opened["issue"]["number"].as_u64(), opened["sender"]["login"].as_str()), (Some(1), Some("bob")));

    // Direct pushes to main are refused by protection, a force push by the owner too.
    let work = git.clone(&server.git_url_as("alice", "demo", "bob", &bob_token));
    git.ok(&work, &["checkout", "--quiet", "-b", "fix-crash"]);
    std::fs::write(work.join("fix.txt"), "no more crash\n").expect("write the fix");
    git.ok(&work, &["add", "--all"]);
    git.ok(&work, &["commit", "--quiet", "-m", "Stop the crash on start", "-m", "refs #1"]);
    let fix = git.ok(&work, &["rev-parse", "HEAD"]).trim().to_owned();
    let refused = git.fails(&work, &["push", "origin", "HEAD:main"]);
    assert!(refused.contains("protected branch: changes must go through a pull request"), "{refused}");
    let alices = git.clone(&server.git_url_as("alice", "demo", "alice", &alice_token));
    git.ok(&alices, &["commit", "--quiet", "--amend", "-m", "Rewritten history"]);
    let refused = git.fails(&alices, &["push", "--force", "origin", "main"]);
    assert!(refused.contains("protected branch") && refused.contains("[remote rejected] main"), "{refused}");
    let main = git.ok(&work, &["ls-remote", "origin", "refs/heads/main"]);
    assert!(main.starts_with(&sample.head), "main is unchanged: {main}");

    // Bob pushes his branch: the commit message's `refs #1` shows on the issue.
    git.ok(&work, &["push", "--quiet", "origin", "fix-crash"]);
    page_with(&alice, "/alice/demo/issues/1", &format!("/alice/demo/commit/{fix}")).await;
    alice.get("/alice/demo/issues/1").await.expect_contains(&["referenced this in commit", "class=\"state-badge state-badge-open\""]);

    // He opens a pull request that fixes the issue and asks alice to review it.
    bob.post(
        "/alice/demo/pulls",
        &[("base", "main"), ("head", "fix-crash"), ("title", "Stop the crash"), ("body", "Fixes #1 by not crashing.")],
    )
    .await
    .expect_redirect("/alice/demo/pull/2");
    let opened = delivery(&inbox, "pull_request", |body| body["action"] == "opened").await;
    assert_eq!((opened["number"].as_u64(), opened["pull_request"]["head"]["ref"].as_str()), (Some(2), Some("fix-crash")));
    page_with(&alice, "/alice/demo/issues/1", "mentioned this in pull request").await;
    alice.get("/alice/demo/issues/1").await.expect_contains(&["Stop the crash", "href=\"/alice/demo/pull/2\""]);
    bob.post("/alice/demo/pull/2/reviewers", &[("reviewer", "alice")]).await.expect_status(StatusCode::SEE_OTHER);
    eventually("alice's review request", || has_unread(&alice)).await;
    alice.get("/notifications").await.expect_contains(&["Stop the crash", "alice/demo"]);
    assert!(!has_unread(&bob).await, "the actor is never notified");

    // Merging is blocked until alice approves (the merge form is not even offered), and
    // the author may not approve his own pull request.
    let pull = page_with(&alice, "/alice/demo/pull/2", "No conflicts with the base branch").await;
    assert!(pull.contains("0 of 1 required approval") && !pull.contains("name=\"expected_head\""), "{pull}");
    bob.post("/alice/demo/pull/2/reviews", &[("verdict", "approve"), ("body", "")]).await.expect_status(StatusCode::BAD_REQUEST);
    let merge = [("strategy", "squash"), ("expected_head", fix.as_str()), ("title", ""), ("message", "")];
    alice.post("/alice/demo/pull/2/merge", &merge).await.expect_status(StatusCode::BAD_REQUEST);
    assert!(git.ok(&work, &["ls-remote", "origin", "refs/heads/main"]).starts_with(&sample.head), "the refused merge changed nothing");

    // Alice reviews: a line comment in a pending review, then the approval.
    alice
        .post(
            "/alice/demo/pull/2/comments",
            &[("path", "fix.txt"), ("side", "right"), ("line", "1"), ("body", "Nice and small"), ("action", "review")],
        )
        .await
        .expect_status(StatusCode::SEE_OTHER);
    alice.post("/alice/demo/pull/2/reviews", &[("verdict", "approve"), ("body", "Ship it")]).await.expect_status(StatusCode::SEE_OTHER);
    let approved = delivery(&inbox, "pull_request_review", |body| body["review"]["state"] == "APPROVED").await;
    assert_eq!(approved["review"]["user"]["login"], "alice");
    let pull = page_with(&alice, "/alice/demo/pull/2", "1 of 1 required approval").await;
    assert!(pull.contains("approved these changes") && pull.contains("Nice and small"), "{pull}");
    let head = rbit_e2e::attribute_after(&pull, "name=\"expected_head\"", "value").expect("the merge form, now");
    assert_eq!(head, fix);
    eventually("bob's review notification", || has_unread(&bob)).await;
    bob.get("/alice/demo/pull/2").await.expect_status(StatusCode::OK);
    assert!(!has_unread(&bob).await, "viewing the pull request marks it read");
    alice.get("/alice/demo/pull/2").await.expect_status(StatusCode::OK);
    assert!(!has_unread(&alice).await, "alice's own review notifies nobody but bob");

    // Alice squash-merges: one new commit on main, whose parent is the old main.
    alice.post("/alice/demo/pull/2/merge", &merge).await.expect_redirect("/alice/demo/pull/2");
    alice.get("/alice/demo/pull/2").await.expect_contains(&["Merged", "Pull request merged"]);
    let merged = git.clone(&server.git_url("alice", "demo"));
    assert_eq!(git.ok(&merged, &["log", "-1", "--format=%s"]).trim(), "Stop the crash (#2)");
    assert_eq!(git.ok(&merged, &["log", "-1", "--format=%P"]).trim(), sample.head, "one parent: the old main");
    assert_eq!(std::fs::read_to_string(merged.join("fix.txt")).expect("the fix is on main").replace('\r', ""), "no more crash\n");
    let squash = git.ok(&merged, &["rev-parse", "HEAD"]).trim().to_owned();
    assert_ne!(squash, fix, "a squash makes a new commit");

    // The merge closed the issue (main is the default branch), with the event on its timeline.
    let issue = page_with(&alice, "/alice/demo/issues/1", "class=\"state-badge state-badge-closed\"").await;
    assert!(issue.contains("closed this as completed in"), "{issue}");
    let closed = delivery(&inbox, "issues", |body| body["action"] == "closed").await;
    assert_eq!((closed["issue"]["number"].as_u64(), closed["issue"]["state"].as_str()), (Some(1), Some("closed")));

    // Bob, the issue's author, hears about it; alice, who merged, does not.
    eventually("bob's notification about the closed issue", || has_unread(&bob)).await;
    bob.get("/notifications").await.expect_contains(&["Crash on start"]);
    assert!(!has_unread(&alice).await, "the merger is never notified");

    // The webhooks: the merged pull request and the push to main it made.
    let merged_hook = delivery(&inbox, "pull_request", |body| body["action"] == "closed").await;
    assert_eq!((merged_hook["pull_request"]["merged"].as_bool(), merged_hook["number"].as_u64()), (Some(true), Some(2)));
    let pushed = delivery(&inbox, "push", |body| body["ref"] == "refs/heads/main").await;
    assert_eq!(pushed["after"].as_str(), Some(squash.as_str()));
    assert_eq!(pushed["before"].as_str(), Some(sample.head.as_str()), "the push moved main from the old tip");
    delivery(&inbox, "push", |body| body["ref"] == "refs/heads/fix-crash").await;
    // Exactly the pushes GitHub would send: not the initial one, made before the hook.
    let mut every_push: Vec<(String, String)> = inbox
        .lock()
        .expect("inbox")
        .iter()
        .filter(|d| d.event == "push")
        .map(|d| (d.body["ref"].as_str().unwrap_or_default().to_owned(), d.body["after"].as_str().unwrap_or_default().to_owned()))
        .collect();
    every_push.sort();
    assert_eq!(every_push, [("refs/heads/fix-crash".to_owned(), fix.clone()), ("refs/heads/main".to_owned(), squash.clone())]);
    assert!(inbox.lock().expect("inbox").iter().all(|d| d.signed), "every delivery is signed");
}
