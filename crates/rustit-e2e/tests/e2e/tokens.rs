//! Personal access tokens as Git passwords: scopes cap access, revocation is immediate.

use rustit_e2e::{Client, Git, StatusCode, TestServer};

/// The revoke URL of the token named `name` on the tokens page.
async fn revoke_url(client: &Client, name: &str) -> String {
    let page = client.get("/settings/tokens").await;
    page.expect_status(StatusCode::OK);
    let label = format!("aria-label=\"Revoke {name}\"");
    let button = page.body.find(&label).unwrap_or_else(|| panic!("no revoke button for {name}"));
    let form = page.body[..button].rfind("action=\"").expect("the button sits in a form") + "action=\"".len();
    let end = form + page.body[form..].find('"').expect("closing quote");
    page.body[form..end].to_owned()
}

#[tokio::test]
async fn token_scopes_cap_git_access_and_revocation_is_immediate() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "vault", "private").await;
    let read = alice.create_token("ci-read", "read").await;
    let write = alice.create_token("laptop-write", "write").await;
    assert_ne!(read, write);

    // The secret is shown once, in a response that must not be cached.
    let listing = alice.get("/settings/tokens").await;
    listing.expect_status(StatusCode::OK).expect_contains(&["ci-read", "laptop-write"]);
    assert!(!listing.contains(&read) && !listing.contains(&write), "secrets are never shown again");

    // A write token pushes (Git's username is ignored for tokens).
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "vault", "anything", &write));

    // A read token clones a private repository but cannot push.
    let clone = git.clone(&server.git_url_as("alice", "vault", "alice", &read));
    assert_eq!(git.ok(&clone, &["rev-parse", "HEAD"]).trim(), sample.head);
    git.commit_file(&clone, "notes.txt", "read tokens cannot push this\n");
    let refused = git.fails(&clone, &["push", &server.git_url_as("alice", "vault", "alice", &read), "HEAD:refs/heads/main"]);
    assert!(refused.contains("403"), "a read token gets 403 on push: {refused}");
    let receive = "/alice/vault.git/info/refs?service=git-receive-pack";
    server.client().get_basic(receive, "alice", &read).await.expect_status(StatusCode::FORBIDDEN);
    git.ok(&clone, &["push", "--quiet", &server.git_url_as("alice", "vault", "alice", &write), "HEAD:refs/heads/main"]);

    // Revoked tokens stop working at once; the other token is unaffected.
    alice.post(&revoke_url(&alice, "ci-read").await, &[]).await.expect_redirect("/settings/tokens");
    let listing = alice.get("/settings/tokens").await;
    assert!(!listing.contains("ci-read"), "the revoked token is gone from the list");
    let upload = "/alice/vault.git/info/refs?service=git-upload-pack";
    server.client().get_basic(upload, "alice", &read).await.expect_status(StatusCode::UNAUTHORIZED);
    git.clone_fails(&server.git_url_as("alice", "vault", "alice", &read));
    git.clone(&server.git_url_as("alice", "vault", "alice", &write));

    // A made-up token is a wrong credential, never a password attempt.
    let forged = format!("rit_{}", "A".repeat(43));
    server.client().get_basic(upload, "alice", &forged).await.expect_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tokens_belong_to_their_owner() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "vault", "private").await;
    alice.create_token("alices-laptop", "write").await;
    let bobs = bob.create_token("bobs", "write").await;

    // Bob's token carries Bob's access, nothing more.
    server
        .client()
        .get_basic("/alice/vault.git/info/refs?service=git-upload-pack", "alice", &bobs)
        .await
        .expect_status(StatusCode::NOT_FOUND);

    // Bob cannot revoke Alice's token, nor see it.
    let url = revoke_url(&alice, "alices-laptop").await;
    bob.post(&url, &[]).await.expect_status(StatusCode::NOT_FOUND);
    assert!(!bob.get("/settings/tokens").await.contains("alices-laptop"));
    alice.get("/settings/tokens").await.expect_contains(&["alices-laptop"]);
}
