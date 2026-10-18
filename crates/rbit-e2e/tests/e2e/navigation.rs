//! Boosted navigation: partial responses between pages of one repository, and the
//! preload guard, through the real router and templates.
//!
//! Frontend virtualization wave, workstream P (preloading), owns this file.

use rbit_e2e::{StatusCode, TestServer};

const PARTIAL: [(&str, &str); 2] = [("HX-Request", "true"), ("Rbit-Partial", "/alice/demo")];

#[tokio::test]
async fn pages_of_the_same_repository_are_sent_partially() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    alice.create_repo("alice", "other", "public").await;

    let full = alice.get_with("/alice/demo/issues", &[("HX-Request", "true")]).await;
    full.expect_status(StatusCode::OK).expect_contains(&["<footer", "id=\"repo-subnav\""]);
    assert!(full.header("rbit-partial").is_none());

    let partial = alice.get_with("/alice/demo/issues", &PARTIAL).await;
    partial.expect_status(StatusCode::OK).expect_contains(&[
        "<main id=\"main\"",
        "<div id=\"repo-subnav\" hx-swap-oob=\"true\" data-repo-scope=\"/alice/demo\"",
        "aria-current=\"page\">Issues",
    ]);
    assert!(partial.body.starts_with("<title>Issues · alice/demo"), "{}", &partial.body[..80.min(partial.body.len())]);
    assert_eq!(partial.header("rbit-partial"), Some("true"));
    for chrome in ["<html", "<footer", "aria-label=\"Main\"", "<!--partial:"] {
        assert!(!partial.contains(chrome), "{chrome} in the partial page");
    }
    assert!(partial.header("vary").is_some_and(|vary| vary.contains("Rbit-Partial") && vary.contains("Rbit-Nav-Epoch")));
    assert_ne!(full.header("etag"), partial.header("etag"), "each shape has its own validator");
    let etag = partial.header("etag").expect("an ETag").to_owned();
    let revalidated = alice.get_with("/alice/demo/issues", &[PARTIAL[0], PARTIAL[1], ("If-None-Match", &etag)]).await;
    revalidated.expect_status(StatusCode::NOT_MODIFIED);

    // A page of another repository, or none, is sent in full.
    for path in ["/alice/other/issues", "/explore", "/alice/demo/does-not-exist"] {
        let response = alice.get_with(path, &PARTIAL).await;
        assert!(response.header("rbit-partial").is_none(), "{path}");
        assert!(response.contains("<footer"), "{path}");
    }
    // So is a page a viewer may not see: the error page, never a slice of anything.
    alice.create_repo("alice", "secret", "private").await;
    let hidden = server.client().get_with("/alice/secret/issues", &[("HX-Request", "true"), ("Rbit-Partial", "/alice/secret")]).await;
    hidden.expect_status(StatusCode::NOT_FOUND);
    assert!(hidden.header("rbit-partial").is_none() && !hidden.contains("repo-subnav"));
}

#[tokio::test]
async fn preloads_stay_fresh_for_the_shared_window_and_never_reach_side_effects() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;

    let preload = alice.get_with("/alice/demo/issues", &[PARTIAL[0], PARTIAL[1], ("HX-Preloaded", "true")]).await;
    preload.expect_status(StatusCode::OK);
    assert_eq!(preload.header("cache-control"), Some("private, max-age=30"));
    assert_eq!(preload.header("rbit-partial"), Some("true"));
    // The page tells the preloader the same window.
    alice.get("/alice/demo").await.expect_contains(&["data-preload-fresh=\"30\"", "data-preload=\"visible\""]);

    let guarded = alice.get_with("/alice/demo/archive/main.zip", &[("HX-Request", "true"), ("HX-Preloaded", "true")]).await;
    guarded.expect_status(StatusCode::NO_CONTENT);
    assert_eq!(guarded.header("cache-control"), Some("no-store"));
}
