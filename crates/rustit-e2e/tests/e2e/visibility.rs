//! Private repositories are invisible to everyone without access, on the web and in Git.

use rustit_e2e::{Git, PASSWORD, StatusCode, TestServer};

#[tokio::test]
async fn private_repository_is_invisible_without_access() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "hidden-plans", "private").await;
    alice.create_repo("alice", "public-notes", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "hidden-plans", "alice", PASSWORD));

    let anonymous = server.client();
    let pages = [
        "/alice/hidden-plans",
        "/alice/hidden-plans/tree/main",
        "/alice/hidden-plans/raw/main/README.md",
        "/alice/hidden-plans/commits",
        "/alice/hidden-plans/branches",
    ];
    for path in pages {
        anonymous.get(path).await.expect_status(StatusCode::NOT_FOUND);
        bob.get(path).await.expect_status(StatusCode::NOT_FOUND);
        alice.get(path).await.expect_status(StatusCode::OK);
    }

    // Listings never mention it.
    for client in [&anonymous, &bob] {
        let profile = client.get("/alice").await;
        profile.expect_status(StatusCode::OK).expect_contains(&["public-notes"]);
        assert!(!profile.contains("hidden-plans"), "the profile hides private repositories");
        let explore = client.get("/explore").await;
        explore.expect_status(StatusCode::OK);
        assert!(!explore.contains("alice/hidden-plans") && !explore.contains("/alice/hidden-plans"), "explore hides private repositories");
    }

    // Git: anonymous callers get a credential challenge (indistinguishable from a
    // missing repository), signed-in callers without access a plain 404.
    let refs = "/alice/hidden-plans.git/info/refs?service=git-upload-pack";
    let challenge = anonymous.get(refs).await;
    challenge.expect_status(StatusCode::UNAUTHORIZED);
    assert!(challenge.header("www-authenticate").is_some_and(|v| v.starts_with("Basic ")));
    let missing = anonymous.get("/alice/no-such-repo.git/info/refs?service=git-upload-pack").await;
    assert_eq!(missing.status, challenge.status, "a private repository looks exactly like a missing one");
    anonymous.get_basic(refs, "bob", PASSWORD).await.expect_status(StatusCode::NOT_FOUND);
    anonymous
        .get_basic("/alice/hidden-plans.git/info/refs?service=git-receive-pack", "bob", PASSWORD)
        .await
        .expect_status(StatusCode::NOT_FOUND);
    anonymous.get_basic(refs, "alice", "wrong-password").await.expect_status(StatusCode::UNAUTHORIZED);
    anonymous.get_basic(refs, "alice", PASSWORD).await.expect_status(StatusCode::OK);

    git.clone_fails(&server.git_url("alice", "hidden-plans"));
    let stderr = git.clone_fails(&server.git_url_as("alice", "hidden-plans", "bob", PASSWORD));
    assert!(stderr.contains("not found") || stderr.contains("404"), "{stderr}");
    let clone = git.clone(&server.git_url_as("alice", "hidden-plans", "alice", PASSWORD));
    assert_eq!(git.ok(&clone, &["rev-parse", "HEAD"]).trim(), sample.head);
}

#[tokio::test]
async fn making_a_repository_private_hides_it_immediately() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    let anonymous = server.client();
    anonymous.get("/alice/demo").await.expect_status(StatusCode::OK);
    anonymous.get("/alice/demo.git/info/refs?service=git-upload-pack").await.expect_status(StatusCode::OK);

    alice
        .post("/alice/demo/settings", &[("description", ""), ("visibility", "private")])
        .await
        .expect_redirect("/alice/demo/settings?saved=1");
    anonymous.get("/alice/demo").await.expect_status(StatusCode::NOT_FOUND);
    anonymous.get("/alice/demo.git/info/refs?service=git-upload-pack").await.expect_status(StatusCode::UNAUTHORIZED);
}
