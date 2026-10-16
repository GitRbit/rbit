//! Collaborators, teams and repository transfer, end to end.
//!
//! Milestone 2 workstream A (access) owns this file.

use rustit_e2e::{Git, PASSWORD, StatusCode, TestServer};

#[tokio::test]
async fn collaborators_get_exactly_their_permission() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let reader = server.sign_up("rita").await;
    let writer = server.sign_up("will").await;
    let outsider = server.sign_up("oscar").await;
    alice.create_repo("alice", "vault", "private").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "vault", "alice", PASSWORD));

    let page = "/alice/vault/settings/collaborators";
    alice.get(page).await.expect_status(StatusCode::OK).expect_contains(&["No collaborators"]);
    alice.post(page, &[("username", "rita"), ("permission", "read")]).await.expect_redirect(page);
    alice.post(page, &[("username", "will"), ("permission", "write")]).await.expect_redirect(page);
    alice
        .post(page, &[("username", "nobody"), ("permission", "read")])
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY)
        .expect_contains(&["There is no user called nobody"]);
    alice.get(page).await.expect_contains(&["rita", "will", "/alice/vault/settings/collaborators/rita/remove"]);

    // A read collaborator browses and clones, but cannot push or see the settings.
    reader.get("/alice/vault").await.expect_status(StatusCode::OK).expect_contains(&["Sample project"]);
    let clone = git.clone(&server.git_url_as("alice", "vault", "rita", PASSWORD));
    assert_eq!(git.ok(&clone, &["rev-parse", "HEAD"]).trim(), sample.head);
    git.commit_file(&clone, "notes.txt", "readers cannot push this\n");
    let refused = git.fails(&clone, &["push", &server.git_url_as("alice", "vault", "rita", PASSWORD), "HEAD:refs/heads/main"]);
    assert!(refused.contains("403"), "a read collaborator gets 403 on push: {refused}");
    reader.get(page).await.expect_status(StatusCode::NOT_FOUND);
    reader.get("/alice/vault/settings").await.expect_status(StatusCode::NOT_FOUND);
    reader.post(page, &[("username", "oscar"), ("permission", "admin")]).await.expect_status(StatusCode::NOT_FOUND);

    // A write collaborator pushes over HTTP with a token.
    let token = writer.create_token("laptop", "write").await;
    git.ok(&clone, &["push", "--quiet", &server.git_url_as("alice", "vault", "will", &token), "HEAD:refs/heads/main"]);
    alice.get("/alice/vault/commits").await.expect_contains(&["Add notes.txt"]);

    // Everyone else still sees nothing.
    let refs = "/alice/vault.git/info/refs?service=git-upload-pack";
    outsider.get("/alice/vault").await.expect_status(StatusCode::NOT_FOUND);
    outsider.get_basic(refs, "oscar", PASSWORD).await.expect_status(StatusCode::NOT_FOUND);
    server.client().get(refs).await.expect_status(StatusCode::UNAUTHORIZED);
    let profile = outsider.get("/alice").await;
    assert!(!profile.contains("vault"), "the profile hides the repository from outsiders");
    reader.get("/").await.expect_contains(&["alice/</span>vault"]);

    // Permissions change and collaborators leave on their own.
    alice.post(&format!("{page}/will/permission"), &[("permission", "read")]).await.expect_redirect(page);
    writer.get_basic("/alice/vault.git/info/refs?service=git-receive-pack", "will", &token).await.expect_status(StatusCode::FORBIDDEN);
    reader.post(&format!("{page}/rita/remove"), &[]).await.expect_redirect("/");
    reader.get("/alice/vault").await.expect_status(StatusCode::NOT_FOUND);
    alice.post(&format!("{page}/will/remove"), &[]).await.expect_redirect(page);
    writer.get_basic(refs, "will", &token).await.expect_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn team_grants_work_like_collaborators() {
    let server = TestServer::start();
    let git = Git::new();
    let owner = server.sign_up("olga").await;
    let member = server.sign_up("mason").await;
    let outsider = server.sign_up("oscar").await;
    owner.create_org("acme").await;
    owner.create_repo("acme", "rocket", "private").await;
    owner.add_member("acme", "mason", "member").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("acme", "rocket", "olga", PASSWORD));

    // With the default base permission members push; with `none` they see nothing.
    member.get("/acme/rocket").await.expect_status(StatusCode::OK);
    owner.post("/acme/settings/permission", &[("permission", "none")]).await.expect_redirect("/acme/settings");
    member.get("/acme/rocket").await.expect_status(StatusCode::NOT_FOUND);
    member.post("/acme/settings/permission", &[("permission", "write")]).await.expect_status(StatusCode::NOT_FOUND);

    // A team grant gives its members access.
    owner.post("/acme/teams/new", &[("name", "core"), ("description", "Rocket engineers")]).await.expect_redirect("/acme/teams/core");
    owner.post("/acme/teams/core/members", &[("username", "mason")]).await.expect_redirect("/acme/teams/core");
    owner
        .post("/acme/teams/core/members", &[("username", "oscar")])
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY)
        .expect_contains(&["not a member of acme"]);
    owner.post("/acme/teams/core/repositories", &[("name", "rocket"), ("permission", "write")]).await.expect_redirect("/acme/teams/core");
    owner.get("/acme/rocket/settings/collaborators").await.expect_contains(&["/acme/teams/core", "Write"]);

    let token = member.create_token("ci", "write").await;
    let clone = git.clone(&server.git_url_as("acme", "rocket", "mason", &token));
    git.commit_file(&clone, "engine.txt", "pushed through a team\n");
    git.ok(&clone, &["push", "--quiet", &server.git_url_as("acme", "rocket", "mason", &token), "HEAD:refs/heads/main"]);

    // Down to read: cloning works, pushing does not.
    owner.post("/acme/teams/core/repositories", &[("name", "rocket"), ("permission", "read")]).await.expect_redirect("/acme/teams/core");
    git.commit_file(&clone, "more.txt", "refused\n");
    let refused = git.fails(&clone, &["push", &server.git_url_as("acme", "rocket", "mason", &token), "HEAD:refs/heads/main"]);
    assert!(refused.contains("403"), "a read team grant gets 403 on push: {refused}");

    // Members see the teams; outsiders and anonymous visitors do not, nor the repository.
    member.get("/acme/teams").await.expect_status(StatusCode::OK).expect_contains(&["core", "1 member · 1 repository"]);
    member.get("/acme/teams/core").await.expect_status(StatusCode::OK).expect_contains(&["rocket", "Leave"]);
    member.post("/acme/teams/core/repositories", &[("name", "rocket"), ("permission", "admin")]).await.expect_status(StatusCode::FORBIDDEN);
    for client in [&outsider, &server.client()] {
        client.get("/acme/teams").await.expect_status(StatusCode::NOT_FOUND);
        client.get("/acme/teams/core").await.expect_status(StatusCode::NOT_FOUND);
        client.get("/acme/rocket").await.expect_status(StatusCode::NOT_FOUND);
    }
    outsider.get_basic("/acme/rocket.git/info/refs?service=git-upload-pack", "oscar", PASSWORD).await.expect_status(StatusCode::NOT_FOUND);

    // Leaving the organisation leaves the team and revokes the access at once.
    member.post("/acme/members/leave", &[]).await.expect_redirect("/");
    member.get_basic("/acme/rocket.git/info/refs?service=git-upload-pack", "mason", &token).await.expect_status(StatusCode::NOT_FOUND);
    owner.get("/acme/teams").await.expect_contains(&["0 members · 1 repository"]);
}

#[tokio::test]
async fn transferred_repositories_clone_from_their_new_address() {
    let server = TestServer::start();
    let git = Git::new();
    // The first account becomes the site administrator, who may transfer anywhere.
    let _root = server.sign_up("root").await;
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_org("acme").await;
    alice.create_repo("alice", "demo", "private").await;
    alice.create_repo("acme", "taken", "private").await;
    alice.create_repo("alice", "taken", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", PASSWORD));

    let settings = alice.get("/alice/demo/settings").await;
    settings.expect_status(StatusCode::OK).expect_contains(&["/alice/demo/settings/transfer", "/alice/demo/settings/collaborators"]);
    alice.get("/alice/demo/settings/transfer").await.expect_status(StatusCode::OK).expect_contains(&[r#"value="acme""#]);
    bob.get("/alice/demo/settings/transfer").await.expect_status(StatusCode::NOT_FOUND);

    // Refusals: wrong confirmation, a target the user may not use, a name clash.
    let transfer = "/alice/demo/settings/transfer";
    alice.post(transfer, &[("new_owner", "acme"), ("confirm", "demo")]).await.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    alice
        .post(transfer, &[("new_owner", "bob"), ("confirm", "alice/demo")])
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY)
        .expect_contains(&["You cannot transfer repositories to bob"]);
    alice
        .post("/alice/taken/settings/transfer", &[("new_owner", "acme"), ("confirm", "alice/taken")])
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY)
        .expect_contains(&["/taken already exists"]);
    alice.get("/alice/taken").await.expect_status(StatusCode::OK);

    alice.post(transfer, &[("new_owner", "acme"), ("confirm", "alice/demo")]).await.expect_redirect("/acme/demo");
    alice.get("/acme/demo").await.expect_status(StatusCode::OK).expect_contains(&["Sample project"]);
    alice.get("/alice/demo").await.expect_status(StatusCode::NOT_FOUND);
    let clone = git.clone(&server.git_url_as("acme", "demo", "alice", PASSWORD));
    assert_eq!(git.ok(&clone, &["rev-parse", "HEAD"]).trim(), sample.head);
    git.clone_fails(&server.git_url_as("alice", "demo", "alice", PASSWORD));
    bob.get("/acme/demo").await.expect_status(StatusCode::NOT_FOUND);
    alice.get("/acme").await.expect_contains(&["demo"]);
}
