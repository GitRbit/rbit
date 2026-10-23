//! Retargeting existing pull requests with the base-branch picker.

use rbit_e2e::{Git, PASSWORD, StatusCode, TestServer};

#[tokio::test]
async fn authors_and_writers_can_retarget_open_pulls_with_the_form() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    let carol = server.sign_up("carol").await;
    alice.create_repo("alice", "demo", "public").await;
    let git = Git::new();
    let sample = git.sample();
    let remote = server.git_url_as("alice", "demo", "alice", PASSWORD);
    git.push_sample(&sample, &remote);
    git.ok(&sample.dir, &["push", &remote, "feature", "main:release"]);
    bob.post("/alice/demo/pulls", &[("base", "main"), ("head", "feature"), ("title", "Retarget me")])
        .await
        .expect_redirect("/alice/demo/pull/1");
    let page = "/alice/demo/pull/1";
    let action = format!("{page}/base");
    for actor in [&alice, &bob] {
        actor.get(page).await.expect_contains(&[
            "Change base branch",
            "Update base branch",
            "data-branches-only=\"true\"",
            "data-repository=\"alice/demo\"",
        ]);
    }
    assert!(!carol.get(page).await.contains("Update base branch"));
    assert!(!server.client().get(page).await.contains("Update base branch"));
    carol.post(&action, &[("base", "release")]).await.expect_status(StatusCode::FORBIDDEN);
    bob.post_with(&action, &[("base", "release")], &[("Origin", "https://evil.example")]).await.expect_status(StatusCode::FORBIDDEN);
    bob.post(&action, &[("base", "does-not-exist")]).await.expect_status(StatusCode::BAD_REQUEST);
    bob.post(&action, &[("base", "feature")]).await.expect_status(StatusCode::BAD_REQUEST);
    bob.post(&action, &[("base", "release")]).await.expect_redirect(page);
    alice.get(page).await.expect_contains(&["value=\"release\"", "changed the base branch from", ">main</code>", ">release</code>"]);
    alice.post(&action, &[("base", "main")]).await.expect_redirect(page);
    alice.post("/alice/demo/issues/1/state", &[("state", "close_completed")]).await.expect_status(StatusCode::SEE_OTHER);
    assert!(!bob.get(page).await.contains("Update base branch"));
    bob.post(&action, &[("base", "release")]).await.expect_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn retargeting_dismisses_approvals_and_refuses_merges_against_the_old_base() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "demo", "public").await;
    let git = Git::new();
    let sample = git.sample();
    let remote = server.git_url_as("alice", "demo", "alice", PASSWORD);
    git.push_sample(&sample, &remote);
    git.ok(&sample.dir, &["push", &remote, "feature", "main:release"]);
    bob.post("/alice/demo/pulls", &[("base", "main"), ("head", "feature"), ("title", "Retarget me")])
        .await
        .expect_redirect("/alice/demo/pull/1");
    let page = "/alice/demo/pull/1";
    alice.post(&format!("{page}/reviews"), &[("verdict", "approve"), ("body", "")]).await.expect_status(StatusCode::SEE_OTHER);

    // The author retargets: the approval for main is dismissed with the change.
    bob.post(&format!("{page}/base"), &[("base", "release")]).await.expect_redirect(page);
    alice.get(page).await.expect_contains(&["Dismissed by bob: The base branch changed from main to release."]);

    // A merger who saw main is refused; the form now carries release.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let body = loop {
        let body = alice.get(page).await.body;
        if body.contains("name=\"expected_base\"") {
            break body;
        }
        assert!(std::time::Instant::now() < deadline, "the merge form never appeared:\n{body}");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };
    assert_eq!(rbit_e2e::attribute_after(&body, "name=\"expected_base\"", "value").as_deref(), Some("release"));
    let head = rbit_e2e::attribute_after(&body, "name=\"expected_head\"", "value").expect("the merge form");
    let merge = |base| [("strategy", "merge"), ("expected_head", head.as_str()), ("expected_base", base), ("title", ""), ("message", "")];
    alice.post(&format!("{page}/merge"), &merge("main")).await.expect_status(StatusCode::BAD_REQUEST);
    alice.post(&format!("{page}/merge"), &merge("release")).await.expect_redirect(page);
}
