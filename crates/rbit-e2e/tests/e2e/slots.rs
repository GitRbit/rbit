//! Deferred slots: secondary data that arrives after the page in one request of
//! out-of-band fragments (frontend wave, workstream S).

use rbit_e2e::{Client, Git, PASSWORD, Sample, StatusCode, TestServer};

const IMMUTABLE: &str = "private, max-age=31536000, immutable";

/// A server where alice has `alice/{name}` with the sample pushed and pull request #1
/// from the sample's feature branch (one commit, one file).
async fn pull_request(server: &TestServer, visibility: &str, name: &str) -> Client {
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", name, visibility).await;
    git.push_sample(&git.sample(), &server.git_url_as("alice", name, "alice", PASSWORD));
    alice
        .post(&format!("/alice/{name}/pulls"), &[("base", "main"), ("head", Sample::BRANCH), ("title", "Feature"), ("body", "")])
        .await
        .expect_redirect(&format!("/alice/{name}/pull/1"));
    alice
}

/// The URL the page's deferred counts come from, if it has one.
fn counts_url(body: &str) -> Option<String> {
    let start = body.find("/pull/1/counts/")?;
    let start = body[..start].rfind('"')? + 1;
    let end = start + body[start..].find('"')?;
    Some(body[start..end].to_owned())
}

#[tokio::test]
async fn pull_request_counts_are_deferred_until_a_diff_has_counted_them() {
    let server = TestServer::start();
    let alice = pull_request(&server, "public", "demo").await;

    // Nothing has diffed yet: the tabs paint at once with slots, filled by one request.
    let conversation = alice.get("/alice/demo/pull/1").await;
    conversation.expect_status(StatusCode::OK).expect_contains(&[r#"id="pull-commits-phrase""#, r#"id="pull-file-count""#]);
    let url = counts_url(&conversation.body).expect("a deferred counts request");
    let fragment = server.client().get(&url).await;
    fragment.expect_status(StatusCode::OK).expect_contains(&[">1 commit</span>", r#"id="pull-file-count""#]);
    assert_eq!(fragment.body.matches(r#"hx-swap-oob="true""#).count(), 3, "{}", fragment.body);
    assert!(!fragment.body.contains("<html"), "only the fragments");
    assert_eq!(fragment.header("cache-control"), Some(IMMUTABLE), "pinned by commit ids");

    // The fragment remembered the counts: the next page has them inline.
    let again = alice.get("/alice/demo/pull/1/commits").await;
    assert_eq!(counts_url(&again.body), None, "{}", again.body);
    again.expect_contains(&[">1 commit</span>"]);

    // Only full commit ids are accepted, and only for a pull request.
    alice.get("/alice/demo/pull/1/counts/main...feature").await.expect_status(StatusCode::NOT_FOUND);
    let (base, head) = url.rsplit_once('/').unwrap().1.split_once("...").unwrap();
    alice.get(&format!("/alice/demo/pull/9/counts/{base}...{head}")).await.expect_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn counts_answer_only_for_the_pull_requests_own_commits() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let sample = git.sample();
    alice.create_repo("alice", "demo", "public").await;
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", PASSWORD));
    alice
        .post("/alice/demo/pulls", &[("base", "main"), ("head", Sample::BRANCH), ("title", "Feature"), ("body", "")])
        .await
        .expect_redirect("/alice/demo/pull/1");

    // A private repository whose pull request head exists nowhere else, counted by its
    // Files changed tab (so the shared cache holds its pair).
    git.ok(&sample.dir, &["checkout", "--quiet", Sample::BRANCH]);
    let secret_head = git.commit_file(&sample.dir, "secret.txt", "classified\n");
    alice.create_repo("alice", "secret", "private").await;
    git.push_sample(&sample, &server.git_url_as("alice", "secret", "alice", PASSWORD));
    alice
        .post("/alice/secret/pulls", &[("base", "main"), ("head", Sample::BRANCH), ("title", "Secret"), ("body", "")])
        .await
        .expect_redirect("/alice/secret/pull/1");
    alice.get("/alice/secret/pull/1/files").await.expect_status(StatusCode::OK).expect_contains(&["secret.txt"]);

    // The public pull request does not answer for the private pair, cached or not.
    let anonymous = server.client();
    let foreign = format!("/alice/demo/pull/1/counts/{}...{secret_head}", sample.head);
    let response = anonymous.get(&foreign).await;
    response.expect_status(StatusCode::NOT_FOUND);
    assert!(!response.contains("pull-file-count"), "no counts leak: {}", response.body);

    // Nor for any other pair of its own repository's commits (here: reversed).
    let url = counts_url(&alice.get("/alice/demo/pull/1").await.body).expect("a deferred counts request");
    let (prefix, pair) = url.rsplit_once('/').unwrap();
    let (base, head) = pair.split_once("...").unwrap();
    anonymous.get(&format!("{prefix}/{head}...{base}")).await.expect_status(StatusCode::NOT_FOUND);
    // Its own pair still answers.
    anonymous.get(&url).await.expect_status(StatusCode::OK).expect_contains(&[">1 commit</span>"]);
}

#[tokio::test]
async fn the_files_tab_counts_for_the_other_tabs() {
    let server = TestServer::start();
    let alice = pull_request(&server, "public", "demo").await;
    alice.get("/alice/demo/pull/1/files").await.expect_status(StatusCode::OK).expect_contains(&["feature.txt", ">1 commit</span>"]);
    let conversation = alice.get("/alice/demo/pull/1").await;
    assert_eq!(counts_url(&conversation.body), None, "counted by the files tab");
}

#[tokio::test]
async fn private_counts_stay_private() {
    let server = TestServer::start();
    let alice = pull_request(&server, "private", "secret").await;
    let url = counts_url(&alice.get("/alice/secret/pull/1").await.body).expect("a deferred counts request");
    server.client().get(&url).await.expect_status(StatusCode::NOT_FOUND);
    let fragment = alice.get(&url).await;
    fragment.expect_status(StatusCode::OK);
    assert_eq!(fragment.header("cache-control"), Some("private, no-cache"), "revalidated, never kept");
}

/// The URL the tree page's last commits come from.
fn tree_commits_url(body: &str) -> String {
    let start = body.find("/tree-commits/").expect("a deferred last-commits request");
    let start = body[..start].rfind('"').unwrap() + 1;
    let end = start + body[start..].find('"').unwrap();
    body[start..end].to_owned()
}

#[tokio::test]
async fn last_commit_fragments_are_immutable_only_when_public() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let sample = git.sample();
    for (name, visibility, expected) in [("open", "public", IMMUTABLE), ("closed", "private", "private, no-cache")] {
        alice.create_repo("alice", name, visibility).await;
        git.push_sample(&sample, &server.git_url_as("alice", name, "alice", PASSWORD));
        let url = tree_commits_url(&alice.get(&format!("/alice/{name}")).await.body);
        assert!(url.contains(&sample.head), "pinned to the commit id: {url}");
        let fragment = alice.get(&url).await;
        fragment.expect_status(StatusCode::OK).expect_contains(&["hx-swap-oob"]);
        assert_eq!(fragment.header("cache-control"), Some(expected), "{name}");
    }
    // A branch name is not pinned: revalidate.
    let by_branch = alice.get("/alice/open/tree-commits/main").await;
    assert_eq!(by_branch.header("cache-control"), Some("private, no-cache"));
}
