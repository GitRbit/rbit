//! Pull requests across forks, reviews and merging, end to end.
//!
//! Milestone 2 workstream P (pull requests) owns this file.
//!
//! Pull request work that follows a push (head refreshes, mergeability) runs in the
//! background, so the tests poll pages until it shows. Webhooks go to a local receiver,
//! so the servers allow private addresses.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{extract::State, routing::post};
use rbit_e2e::{Client, Git, PASSWORD, Response, StatusCode, TestServer};
use serde_json::{Value, json};

fn server() -> TestServer {
    TestServer::start_with(|config| config.webhooks.allow_private_addresses = true)
}

/// Fetches `path` until `ready` holds for its body (background work), at most 30 s.
async fn wait_for_page(client: &Client, path: &str, ready: impl Fn(&str) -> bool) -> Response {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let response = client.get(path).await;
        if ready(&response.body) {
            return response;
        }
        assert!(Instant::now() < deadline, "{path} never became ready:\n{}", response.body);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

type Inbox = Arc<Mutex<Vec<(String, Value)>>>;

/// Records every webhook delivery as `(event, body)`.
async fn receiver() -> (String, Inbox) {
    async fn receive(State(inbox): State<Inbox>, headers: axum::http::HeaderMap, body: String) -> StatusCode {
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

async fn wait_for_delivery(inbox: &Inbox, matches: impl Fn(&str, &Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some((_, body)) = inbox.lock().expect("inbox").iter().find(|(event, body)| matches(event, body)) {
            return body.clone();
        }
        assert!(Instant::now() < deadline, "the expected delivery never arrived: {:?}", inbox.lock().expect("inbox"));
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A plain token-authenticated API client.
struct Api {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl Api {
    fn new(server: &TestServer, token: String) -> Self {
        let http = reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).build().expect("API client");
        Self { http, base: server.url("/api/v1"), token }
    }

    async fn send(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut request =
            self.http.request(method, format!("{}{path}", self.base)).header("authorization", format!("Bearer {}", self.token));
        if let Some(body) = body {
            request = request.header("content-type", "application/json").body(body.to_string());
        }
        let response = request.send().await.expect("the API answers");
        let status = response.status();
        let text = response.text().await.expect("read the body");
        (status, if text.is_empty() { Value::Null } else { serde_json::from_str(&text).expect("JSON") })
    }
}

/// The first `id="diff-…-R{line}"` anchor of the Files changed page.
fn line_anchor(body: &str, line: u32) -> String {
    let suffix = format!("-R{line}\"");
    let end = body.find(&suffix).expect("a right-side line anchor");
    let start = body[..end].rfind("id=\"diff-").expect("the anchor's start") + "id=\"".len();
    body[start..end + suffix.len() - 1].to_owned()
}

fn assert_review_file(files: &Response) {
    files.expect_status(StatusCode::OK).expect_contains(&[
        "greeting.txt",
        "?comment=diff-",
        "data-diff-layout",
        "class=\"diff-file box group\" data-path=\"greeting.txt\" data-rows=\"",
    ]);
}

async fn assert_pull_search(alice: &Client) {
    alice.get("/alice/demo/pulls").await.expect_contains(&["Greet the world", "1</span> Open", "Search pull requests", "Find label"]);
    alice.get("/alice/demo/pulls?q=world%20author%3Abob").await.expect_contains(&["Greet the world"]);
    let empty = alice.get("/alice/demo/pulls?q=world%20author%3Aunknown").await;
    assert!(!empty.contains("href=\"/alice/demo/pull/1\""));
    alice.get("/alice/demo/pulls?state=all&author=bob").await.expect_contains(&["Greet the world"]);
}

#[tokio::test]
async fn a_fork_pull_request_is_reviewed_fixed_approved_and_squash_merged() {
    let server = server();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", PASSWORD));
    let (hook_url, inbox) = receiver().await;
    alice
        .post(
            "/alice/demo/settings/hooks",
            &[("url", &hook_url), ("event_pull_request", "on"), ("event_pull_request_review", "on"), ("active", "on")],
        )
        .await
        .expect_status(StatusCode::SEE_OTHER);

    // Bob forks, pushes a branch to his fork and opens a pull request to the parent.
    let bob = server.sign_up("bob").await;
    bob.post("/alice/demo/fork", &[("owner", "bob"), ("name", "")]).await.expect_redirect("/bob/demo");
    let work = git.clone(&server.git_url_as("bob", "demo", "bob", PASSWORD));
    git.ok(&work, &["checkout", "--quiet", "-b", "greeting"]);
    git.commit_file(&work, "greeting.txt", "hello\nfrom bob\n");
    git.ok(&work, &["push", "--quiet", "origin", "greeting"]);

    bob.get("/alice/demo/compare/main...bob:greeting").await.expect_status(StatusCode::OK).expect_contains(&[
        "Create pull request",
        "greeting.txt",
        "bob/demo",
    ]);
    bob.get("/alice/demo/pulls/new?base=main&head=bob%3Agreeting")
        .await
        .expect_status(StatusCode::OK)
        .expect_contains(&["Open a pull request", "Add greeting.txt"]);
    let created = bob
        .post(
            "/alice/demo/pulls",
            &[("base", "main"), ("head", "bob:greeting"), ("title", "Greet the world"), ("body", "Adds a greeting.")],
        )
        .await;
    created.expect_redirect("/alice/demo/pull/1");
    bob.post("/alice/demo/pulls", &[("base", "main"), ("head", "bob:greeting"), ("title", "Again")])
        .await
        .expect_status(StatusCode::BAD_REQUEST)
        .expect_contains(&["already proposes"]);
    alice.get("/alice/demo/pull/1").await.expect_status(StatusCode::OK).expect_contains(&[
        "Greet the world",
        "wants to merge",
        "bob:greeting",
        "id=\"merge-box\"",
    ]);
    assert_pull_search(&alice).await;
    wait_for_delivery(&inbox, |event, body| event == "pull_request" && body["action"] == "opened").await;

    // Alice comments on a line and requests changes.
    let files = alice.get("/alice/demo/pull/1/files").await;
    assert_review_file(&files);
    let anchor = line_anchor(&files.body, 2);
    alice.get(&format!("/alice/demo/pull/1/files?comment={anchor}")).await.expect_contains(&[
        "Add single comment",
        "Start a review",
        "<div class=\"diff-comment\">",
    ]);
    alice
        .post(
            "/alice/demo/pull/1/comments",
            &[("path", "greeting.txt"), ("side", "right"), ("line", "2"), ("body", "Say who you are"), ("action", "single")],
        )
        .await
        .expect_redirect(&format!("/alice/demo/pull/1/files#{anchor}"));
    let review = alice.post("/alice/demo/pull/1/reviews", &[("verdict", "request_changes"), ("body", "Please sign it")]).await;
    review.expect_status(StatusCode::SEE_OTHER);
    assert!(review.location().unwrap().contains("#pullrequestreview-"));
    bob.post("/alice/demo/pull/1/reviews", &[("verdict", "approve"), ("body", "")]).await.expect_status(StatusCode::BAD_REQUEST);
    alice.get("/alice/demo/pull/1").await.expect_contains(&["requested changes", "Please sign it", "Say who you are"]);
    wait_for_delivery(&inbox, |event, body| event == "pull_request_review" && body["review"]["state"] == "CHANGES_REQUESTED").await;

    // Bob pushes a fix: the timeline shows it and the comment is outdated.
    git.commit_file(&work, "greeting.txt", "hello\nfrom bob, the author\n");
    git.ok(&work, &["push", "--quiet", "origin", "greeting"]);
    wait_for_page(&alice, "/alice/demo/pull/1", |body| body.contains("added 1 commit")).await;
    wait_for_page(&alice, "/alice/demo/pull/1/files", |body| body.contains("1 outdated comment")).await;
    alice.get("/alice/demo/pull/1").await.expect_contains(&["Outdated"]);
    wait_for_delivery(&inbox, |event, body| event == "pull_request" && body["action"] == "synchronize").await;

    // Alice approves and squash-merges.
    alice.post("/alice/demo/pull/1/reviews", &[("verdict", "approve"), ("body", "Thanks!")]).await.expect_status(StatusCode::SEE_OTHER);
    let page = wait_for_page(&alice, "/alice/demo/pull/1", |body| body.contains("No conflicts with the base branch")).await;
    let head = rbit_e2e::attribute_after(&page.body, "name=\"expected_head\"", "value").expect("the merge form");
    alice
        .post("/alice/demo/pull/1/merge", &[("strategy", "squash"), ("expected_head", &head), ("title", ""), ("message", "")])
        .await
        .expect_redirect("/alice/demo/pull/1");
    alice.get("/alice/demo/pull/1").await.expect_contains(&["Merged", "Pull request merged"]);

    // A clone of the parent has the squash commit, written with a noreply identity.
    let parent = git.clone(&server.git_url("alice", "demo"));
    assert_eq!(git.ok(&parent, &["log", "-1", "--format=%s"]).trim(), "Greet the world (#1)");
    assert_eq!(git.ok(&parent, &["log", "-1", "--format=%P"]).trim(), sample.head, "one parent: the old main");
    let identities = git.ok(&parent, &["log", "-1", "--format=%an <%ae> %cn <%ce>"]);
    assert!(identities.contains("@noreply.127.0.0.1>") && !identities.contains("@example.test"), "{identities}");
    assert_eq!(std::fs::read_to_string(parent.join("greeting.txt")).unwrap().replace('\r', ""), "hello\nfrom bob, the author\n");

    let closed = wait_for_delivery(&inbox, |event, body| event == "pull_request" && body["action"] == "closed").await;
    assert_eq!(closed["pull_request"]["merged"], true);
    assert_eq!(closed["pull_request"]["head"]["label"], "bob:greeting");

    // Bob may delete his merged branch; a reader may not (alice is the site administrator).
    bob.get("/alice/demo/pull/1").await.expect_contains(&["Delete bob:greeting"]);
    server.sign_up("carol").await.post("/alice/demo/pull/1/branch/delete", &[]).await.expect_status(StatusCode::FORBIDDEN);
    bob.post("/alice/demo/pull/1/branch/delete", &[]).await.expect_redirect("/alice/demo/pull/1");
    let branches = git.ok(&work, &["ls-remote", "--heads", "origin"]);
    assert!(!branches.contains("refs/heads/greeting"), "{branches}");
    // The pull request keeps its commits without the branch.
    alice.get("/alice/demo/pull/1/commits").await.expect_status(StatusCode::OK).expect_contains(&["Add greeting.txt"]);
    // Reopening a merged pull request is refused.
    alice.post("/alice/demo/issues/1/state", &[("state", "reopen")]).await.expect_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one scenario: conflicts, the API, redirects and privacy")]
async fn conflicts_api_merges_and_hidden_repositories() {
    let server = server();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    let sample = git.sample();
    let url = server.git_url_as("alice", "demo", "alice", PASSWORD);
    git.push_sample(&sample, &url);
    // `clash` and main both edit README.md; `rebase-me` and `merge-me` are clean.
    for branch in ["clash", "rebase-me", "merge-me"] {
        git.ok(&sample.dir, &["checkout", "--quiet", "-b", branch, "main"]);
        if branch == "clash" {
            git.commit_file(&sample.dir, "README.md", "# Clash\n");
        } else {
            git.commit_file(&sample.dir, &format!("{branch}.txt"), "work\n");
        }
        git.ok(&sample.dir, &["push", "--quiet", &url, branch]);
    }
    git.ok(&sample.dir, &["checkout", "--quiet", "main"]);
    git.commit_file(&sample.dir, "README.md", "# Changed on main\n");
    git.ok(&sample.dir, &["push", "--quiet", &url, "main"]);

    // A conflicting pull request lists the file and refuses to merge.
    alice.post("/alice/demo/pulls", &[("base", "main"), ("head", "clash"), ("title", "Clash")]).await.expect_redirect("/alice/demo/pull/1");
    wait_for_page(&alice, "/alice/demo/pull/1", |body| body.contains("Conflicting files"))
        .await
        .expect_contains(&["README.md", "conflicts that must be resolved"]);
    alice.post("/alice/demo/pull/1/merge", &[("strategy", "merge")]).await.expect_status(StatusCode::BAD_REQUEST);

    // The API: open, list, merge with each method, 405 and 409.
    let token = alice.create_token("ci", "write").await;
    let api = Api::new(&server, token);
    let (status, opened) =
        api.send(reqwest::Method::POST, "/repos/alice/demo/pulls", Some(json!({ "head": "rebase-me", "base": "main" }))).await;
    assert_eq!(status, StatusCode::CREATED, "{opened}");
    assert_eq!((opened["number"].as_u64(), opened["title"].as_str()), (Some(2), Some("Add rebase-me.txt")));
    let (_, merged_pull) = api
        .send(reqwest::Method::POST, "/repos/alice/demo/pulls", Some(json!({ "head": "merge-me", "base": "main", "title": "Merge me" })))
        .await;
    assert_eq!(merged_pull["number"], 3);
    let (status, list) = api.send(reqwest::Method::GET, "/repos/alice/demo/pulls?state=open&sort=created&direction=asc", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().map(Vec::len), Some(3));
    let (_, files) = api.send(reqwest::Method::GET, "/repos/alice/demo/pulls/2/files", None).await;
    assert_eq!(files[0]["filename"], "rebase-me.txt");
    assert!(files[0]["patch"].as_str().unwrap().contains("+work"));

    let (status, _) = api.send(reqwest::Method::PUT, "/repos/alice/demo/pulls/1/merge", Some(json!({}))).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    let (status, _) = api
        .send(reqwest::Method::PUT, "/repos/alice/demo/pulls/2/merge", Some(json!({ "merge_method": "rebase", "sha": "0".repeat(40) })))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = api.send(reqwest::Method::GET, "/repos/alice/demo/pulls/2/merge", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, rebased) =
        api.send(reqwest::Method::PUT, "/repos/alice/demo/pulls/2/merge", Some(json!({ "merge_method": "rebase" }))).await;
    assert_eq!(status, StatusCode::OK, "{rebased}");
    assert_eq!(rebased["merged"], true);
    let (status, _) = api.send(reqwest::Method::GET, "/repos/alice/demo/pulls/2/merge", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, merge_commit) =
        api.send(reqwest::Method::PUT, "/repos/alice/demo/pulls/3/merge", Some(json!({ "merge_method": "merge" }))).await;
    assert_eq!(status, StatusCode::OK, "{merge_commit}");
    let parent = git.clone(&server.git_url("alice", "demo"));
    assert_eq!(git.ok(&parent, &["log", "-1", "--format=%s"]).trim(), "Merge pull request #3 from alice/merge-me");
    assert_eq!(git.ok(&parent, &["rev-list", "--count", "--merges", "HEAD"]).trim(), "1");
    assert_eq!(
        git.ok(&parent, &["log", "-1", "--format=%s", "HEAD^1"]).trim(),
        "Add rebase-me.txt",
        "the rebased commit sits under the merge"
    );
    let (_, shown) = api.send(reqwest::Method::GET, "/repos/alice/demo/pulls/3", None).await;
    assert_eq!((shown["merged"].as_bool(), shown["state"].as_str()), (Some(true), Some("closed")));

    // Issue numbers under /pull redirect (issues are seeded through the store: their pages are I's).
    let database = rbit_storage::Database::open(&server.data_dir().join("rbit.db")).await.unwrap();
    let author = database.accounts().find_account_by_name("alice").await.unwrap().unwrap();
    let repo = database.repositories().find_by_path("alice", "demo").await.unwrap().unwrap();
    let issue = rbit_storage::sqlite::issues::NewIssue {
        repo: repo.repo.id,
        kind: rbit_core::IssueKind::Issue,
        title: rbit_core::IssueTitle::parse("A bug").unwrap(),
        body: rbit_core::IssueText::body("").unwrap(),
        author: author.id,
        at: rbit_core::Timestamp::now(),
    };
    let number = database.issues().create(&issue).await.unwrap().number.get();
    database.close().await;
    let moved = alice.get(&format!("/alice/demo/pull/{number}/files")).await;
    moved.expect_status(StatusCode::MOVED_PERMANENTLY);
    assert_eq!(moved.location(), Some(format!("/alice/demo/issues/{number}").as_str()));
    let moved = alice.get(&format!("/alice/demo/pull/{number}")).await;
    moved.expect_status(StatusCode::MOVED_PERMANENTLY);
    assert_eq!(moved.location(), Some(format!("/alice/demo/issues/{number}").as_str()));
    // And the other way: a pull request number under /issues is the same true 301.
    let moved = alice.get("/alice/demo/issues/2").await;
    moved.expect_status(StatusCode::MOVED_PERMANENTLY);
    assert_eq!(moved.location(), Some("/alice/demo/pull/2"));
    let (status, _) = api.send(reqwest::Method::GET, &format!("/repos/alice/demo/pulls/{number}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "an issue is not a pull request");

    // A private repository's pull requests are 404 for outsiders, pages and API alike.
    alice.create_repo("alice", "secret", "private").await;
    let secret_url = server.git_url_as("alice", "secret", "alice", PASSWORD);
    git.push_sample(&sample, &secret_url);
    alice
        .post("/alice/secret/pulls", &[("base", "main"), ("head", "feature"), ("title", "Secret work")])
        .await
        .expect_redirect("/alice/secret/pull/1");
    let mallory = server.sign_up("mallory").await;
    for path in [
        "/alice/secret/pulls",
        "/alice/secret/pull/1",
        "/alice/secret/pull/1/files",
        "/alice/secret/pull/1/commits",
        "/alice/secret/issues/1",
        "/alice/secret/pulls/new?head=feature",
        "/alice/demo/pull/99",
        "/alice/demo/pull/abc",
    ] {
        mallory.get(path).await.expect_status(StatusCode::NOT_FOUND);
    }
    mallory.post("/alice/secret/pull/1/merge", &[("strategy", "merge")]).await.expect_status(StatusCode::NOT_FOUND);
    mallory.post("/alice/secret/pull/1/reviews", &[("verdict", "approve")]).await.expect_status(StatusCode::NOT_FOUND);
    mallory.post("/alice/demo/pull/1/merge", &[("strategy", "merge")]).await.expect_status(StatusCode::FORBIDDEN);
    let mallory_api = Api::new(&server, mallory.create_token("probe", "write").await);
    for path in ["/repos/alice/secret/pulls", "/repos/alice/secret/pulls/1", "/repos/alice/secret/pulls/1/reviews"] {
        assert_eq!(mallory_api.send(reqwest::Method::GET, path, None).await.0, StatusCode::NOT_FOUND, "{path}");
    }
    let foreign = mallory
        .post_with("/alice/demo/pull/1/reviews", &[("verdict", "comment"), ("body", "csrf")], &[("Origin", "http://evil.example")])
        .await;
    foreign.expect_status(StatusCode::FORBIDDEN);
}
