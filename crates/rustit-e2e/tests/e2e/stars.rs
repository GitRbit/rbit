//! Stars: starring and unstarring update the count and the stargazer list.

use rustit_e2e::{Client, StatusCode, TestServer};

/// The star count shown on the repository page.
async fn star_count(client: &Client, repo: &str) -> usize {
    let page = client.get(repo).await;
    page.expect_status(StatusCode::OK);
    let marker = "title=\"Stargazers\" aria-label=\"";
    let start = page.body.find(marker).expect("the star button is rendered") + marker.len();
    let digits: String = page.body[start..].chars().take_while(char::is_ascii_digit).collect();
    digits.parse().expect("a numeric star count")
}

#[tokio::test]
async fn starring_updates_the_count() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "demo", "public").await;
    assert_eq!(star_count(&bob, "/alice/demo").await, 0);

    bob.post("/alice/demo/star", &[("action", "star")]).await.expect_redirect("/alice/demo");
    assert_eq!(star_count(&bob, "/alice/demo").await, 1);
    // Starring twice is idempotent.
    bob.post("/alice/demo/star", &[("action", "star")]).await.expect_redirect("/alice/demo");
    assert_eq!(star_count(&server.client(), "/alice/demo").await, 1);
    bob.get("/alice/demo/stargazers").await.expect_status(StatusCode::OK).expect_contains(&["/bob"]);

    // htmx gets the updated button as a fragment.
    let fragment = alice.post_with("/alice/demo/star", &[("action", "star")], &[("HX-Request", "true")]).await;
    fragment.expect_status(StatusCode::OK).expect_contains(&["id=\"star-button\"", "aria-label=\"2 stargazers\""]);
    assert!(!fragment.contains("<html"), "a fragment, not a page");
    assert_eq!(star_count(&alice, "/alice/demo").await, 2);

    bob.post("/alice/demo/star", &[("action", "unstar")]).await.expect_redirect("/alice/demo");
    assert_eq!(star_count(&alice, "/alice/demo").await, 1);
    let stargazers = alice.get("/alice/demo/stargazers").await;
    assert!(!stargazers.contains("href=\"/bob\""), "bob is no longer a stargazer");

    // Anonymous visitors are sent to sign in and change nothing.
    let anonymous = server.client().post("/alice/demo/star", &[("action", "star")]).await;
    anonymous.expect_status(StatusCode::SEE_OTHER);
    assert!(anonymous.location().is_some_and(|l| l.starts_with("/login?next=")));
    assert_eq!(star_count(&alice, "/alice/demo").await, 1);
}

#[tokio::test]
async fn private_repositories_cannot_be_starred_by_outsiders() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "hidden", "private").await;
    bob.post("/alice/hidden/star", &[("action", "star")]).await.expect_status(StatusCode::NOT_FOUND);
    bob.get("/alice/hidden/stargazers").await.expect_status(StatusCode::NOT_FOUND);
    assert_eq!(star_count(&alice, "/alice/hidden").await, 0);
}
