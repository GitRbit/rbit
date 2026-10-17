//! Old names of renamed and transferred repositories: web pages redirect, Git over HTTP
//! keeps working, a new repository at the old name wins, and readers only.

use rustit_e2e::{Git, PASSWORD, StatusCode, TestServer};

#[tokio::test]
async fn renamed_repositories_redirect_from_their_old_name() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "before", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "before", "alice", PASSWORD));
    alice.post("/alice/before/settings/rename", &[("name", "after")]).await.expect_redirect("/alice/after/settings");

    // Every page under the old name, for everyone who may read it, keeping path and query.
    let anonymous = server.client();
    for (old, new) in [
        ("/alice/before", "/alice/after"),
        ("/Alice/Before/", "/alice/after/"),
        ("/alice/before/blob/main/README.md", "/alice/after/blob/main/README.md"),
        ("/alice/before/commits/main?page=2", "/alice/after/commits/main?page=2"),
        ("/alice/before/settings", "/alice/after/settings"),
        ("/alice/before/archive/main.zip", "/alice/after/archive/main.zip"),
    ] {
        let moved = anonymous.get(old).await;
        moved.expect_status(StatusCode::MOVED_PERMANENTLY);
        assert_eq!(moved.location(), Some(new), "{old} redirects");
        assert_eq!(moved.header("cache-control"), Some("no-store"), "a later repository may take the name");
    }
    // Following the redirect lands on the repository.
    alice.get("/alice/after/blob/main/README.md").await.expect_status(StatusCode::OK);
    // Other methods keep theirs.
    let star = alice.post("/alice/before/star", &[("action", "star")]).await;
    star.expect_status(StatusCode::PERMANENT_REDIRECT);
    assert_eq!(star.location(), Some("/alice/after/star"));
    // Names that never existed are still not found.
    anonymous.get("/alice/never").await.expect_status(StatusCode::NOT_FOUND);
    anonymous.get("/nobody/before").await.expect_status(StatusCode::NOT_FOUND);

    // Git remotes configured with the old name keep working for fetch and push.
    let clone = git.clone(&server.git_url("alice", "before"));
    assert_eq!(git.ok(&clone, &["rev-parse", "HEAD"]).trim(), sample.head);
    let pushed = git.commit_file(&clone, "renamed.txt", "still here\n");
    git.ok(&clone, &["push", "--quiet", &server.git_url_as("alice", "before", "alice", PASSWORD), "HEAD:refs/heads/main"]);
    alice.get("/alice/after/commits/main").await.expect_contains(&[&pushed[..7]]);

    // A second rename: both old names lead to the current one.
    alice.post("/alice/after/settings/rename", &[("name", "latest")]).await.expect_redirect("/alice/latest/settings");
    for old in ["/alice/before", "/alice/after"] {
        assert_eq!(anonymous.get(old).await.location(), Some("/alice/latest"), "{old} redirects");
    }
}

#[tokio::test]
async fn a_new_repository_at_the_old_name_takes_precedence() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "app", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "app", "alice", PASSWORD));
    alice.post("/alice/app/settings/rename", &[("name", "app-legacy")]).await.expect_redirect("/alice/app-legacy/settings");
    alice.get("/alice/app").await.expect_status(StatusCode::MOVED_PERMANENTLY);

    alice.create_repo("alice", "app", "public").await;
    alice.get("/alice/app").await.expect_status(StatusCode::OK).expect_contains(&["alice/app"]);
    // The new, empty repository is what Git sees at the name now.
    let refs = server.client().get("/alice/app.git/info/refs?service=git-upload-pack").await;
    refs.expect_status(StatusCode::OK);
    assert!(!refs.contains(&sample.head), "the old repository is no longer served at its old name");
    // Its own missing pages are plain "not found", not redirects to the old repository.
    alice.get("/alice/app/blob/main/README.md").await.expect_status(StatusCode::NOT_FOUND);
    alice.get("/alice/app-legacy/blob/main/README.md").await.expect_status(StatusCode::OK);
}

#[tokio::test]
async fn private_repositories_redirect_only_for_their_readers() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "secret", "private").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "secret", "alice", PASSWORD));
    alice.post("/alice/secret/settings/rename", &[("name", "hidden")]).await.expect_redirect("/alice/hidden/settings");

    let moved = alice.get("/alice/secret/commits/main").await;
    moved.expect_status(StatusCode::MOVED_PERMANENTLY);
    assert_eq!(moved.location(), Some("/alice/hidden/commits/main"));

    // Everyone else gets the same answer as for a name that never existed: the new name
    // does not leak.
    for client in [&bob, &server.client()] {
        let page = client.get("/alice/secret").await;
        page.expect_status(StatusCode::NOT_FOUND);
        assert_eq!(page.location(), None);
        assert!(!page.contains("alice/hidden"));
    }
    let as_bob = server.client().get_basic("/alice/secret.git/info/refs?service=git-upload-pack", "bob", PASSWORD).await;
    as_bob.expect_status(StatusCode::NOT_FOUND);
    let anonymous = server.client().get("/alice/secret.git/info/refs?service=git-upload-pack").await;
    anonymous.expect_status(StatusCode::UNAUTHORIZED);

    // The owner's existing remotes keep working.
    let clone = git.clone(&server.git_url_as("alice", "secret", "alice", PASSWORD));
    assert_eq!(git.ok(&clone, &["rev-parse", "HEAD"]).trim(), sample.head);
}

#[tokio::test]
async fn transferred_repositories_redirect_from_their_old_owner() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_org("acme").await;
    alice.create_repo("alice", "demo", "public").await;
    // Transferring right after a push: the push's background work must not hold the
    // repository's files open (Windows cannot move a directory with open files).
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", PASSWORD));
    alice.post("/alice/demo/settings/transfer", &[("new_owner", "acme"), ("confirm", "alice/demo")]).await.expect_redirect("/acme/demo");

    // Pushing to and cloning from the old location reach the repository.
    let pushed = git.commit_file(
        &sample.dir,
        "moved.txt",
        "pushed after the transfer
",
    );
    git.ok(&sample.dir, &["push", "--quiet", &server.git_url_as("alice", "demo", "alice", PASSWORD), "HEAD:refs/heads/main"]);
    let clone = git.clone(&server.git_url("alice", "demo"));
    assert_eq!(git.ok(&clone, &["rev-parse", "HEAD"]).trim(), pushed);
    let moved = server.client().get("/alice/demo/tree/main/src").await;
    moved.expect_status(StatusCode::MOVED_PERMANENTLY);
    assert_eq!(moved.location(), Some("/acme/demo/tree/main/src"));
    server.client().get("/acme/demo/tree/main/src").await.expect_status(StatusCode::OK);

    // Alice can reuse the name; her new repository wins.
    alice.create_repo("alice", "demo", "public").await;
    alice.get("/alice/demo").await.expect_status(StatusCode::OK);
}
