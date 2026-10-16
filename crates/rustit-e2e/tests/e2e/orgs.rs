//! Organisations: members can push to organisation repositories, outsiders see nothing.

use rustit_e2e::{Git, PASSWORD, StatusCode, TestServer};

#[tokio::test]
async fn members_push_and_outsiders_cannot_see_private_org_repositories() {
    let server = TestServer::start();
    let git = Git::new();
    let owner = server.sign_up("olivia").await;
    let member = server.sign_up("mason").await;
    let outsider = server.sign_up("oscar").await;

    owner.create_org("acme").await;
    owner.create_repo("acme", "rocket", "private").await;
    owner.create_repo("acme", "brochure", "public").await;
    owner.add_member("acme", "mason", "member").await;
    owner.get("/acme/members").await.expect_status(StatusCode::OK).expect_contains(&["mason"]);

    // A member reads and pushes over Git with their own password.
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("acme", "rocket", "mason", PASSWORD));
    member.get("/acme/rocket").await.expect_status(StatusCode::OK).expect_contains(&["Sample project"]);
    member.get("/acme").await.expect_status(StatusCode::OK).expect_contains(&["rocket", "brochure"]);
    // ... and may create repositories in the organisation, but not administer them.
    member.create_repo("acme", "sidecar", "private").await;
    member.get("/acme/rocket/settings").await.expect_status(StatusCode::NOT_FOUND);

    // Outsiders (signed in or not) see neither the repository nor its name.
    for client in [&outsider, &server.client()] {
        client.get("/acme/rocket").await.expect_status(StatusCode::NOT_FOUND);
        client.get("/acme/rocket/commits").await.expect_status(StatusCode::NOT_FOUND);
        let profile = client.get("/acme").await;
        profile.expect_status(StatusCode::OK).expect_contains(&["brochure"]);
        assert!(!profile.contains("rocket") && !profile.contains("sidecar"), "private organisation repositories are hidden");
    }
    let refs = "/acme/rocket.git/info/refs?service=git-upload-pack";
    outsider.get_basic(refs, "oscar", PASSWORD).await.expect_status(StatusCode::NOT_FOUND);
    git.clone_fails(&server.git_url_as("acme", "rocket", "oscar", PASSWORD));
    let refused = git.fails(&sample.dir, &["push", &server.git_url_as("acme", "rocket", "oscar", PASSWORD), "main:refs/heads/oscar"]);
    assert!(refused.contains("404") || refused.contains("not found"), "{refused}");
    // Outsiders cannot create repositories in the organisation or manage its members.
    let page = outsider.post("/new", &[("owner", "acme"), ("name", "intruder"), ("visibility", "public")]).await;
    page.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    // Members cannot manage members (403); outsiders are not told the roster is managed (404).
    outsider.post("/acme/members", &[("username", "oscar"), ("role", "owner")]).await.expect_status(StatusCode::NOT_FOUND);
    member.post("/acme/members", &[("username", "oscar"), ("role", "member")]).await.expect_status(StatusCode::FORBIDDEN);
    let roster = owner.get("/acme/members").await;
    roster.expect_status(StatusCode::OK);
    assert!(!roster.contains("oscar"), "nobody was added");

    // Removing the member revokes their access at once.
    owner.post("/acme/members/mason/remove", &[]).await.expect_redirect("/acme/members");
    member.get("/acme/rocket").await.expect_status(StatusCode::NOT_FOUND);
    member.get_basic(refs, "mason", PASSWORD).await.expect_status(StatusCode::NOT_FOUND);
}
