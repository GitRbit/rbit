//! Resolving submitted review conversations through the real web forms.

use rbit_e2e::{Git, PASSWORD, StatusCode, TestServer, attribute_after};

#[tokio::test]
async fn resolution_forms_preserve_threads_permissions_and_timeline() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    let carol = server.sign_up("carol").await;
    alice.create_repo("alice", "demo", "public").await;
    let git = Git::new();
    let sample = git.sample();
    let remote = server.git_url_as("alice", "demo", "alice", PASSWORD);
    git.push_sample(&sample, &remote);
    git.ok(&sample.dir, &["remote", "add", "origin", &remote]);
    git.ok(&sample.dir, &["checkout", "-b", "review-resolution", "main"]);
    git.commit_file(&sample.dir, "feature.txt", "one\ntwo\n");
    git.ok(&sample.dir, &["push", "origin", "review-resolution"]);
    alice
        .post("/alice/demo/pulls", &[("base", "main"), ("head", "review-resolution"), ("title", "Review me")])
        .await
        .expect_redirect("/alice/demo/pull/1");
    let page = "/alice/demo/pull/1";
    let files = format!("{page}/files");
    bob.post(&format!("{page}/comments"), &[("path", "feature.txt"), ("side", "right"), ("line", "1"), ("body", "Explain this")])
        .await
        .expect_status(StatusCode::SEE_OTHER);
    let body = bob.get(&files).await.body;
    // Extract the root from the reply form; resolution controls use this same id.
    let root = attribute_after(&body, "name=\"in_reply_to\"", "value").unwrap();
    let resolution = format!("{page}/comments/{root}/resolution");
    assert!(body.contains(&resolution));
    carol.post(&format!("{page}/comments"), &[("in_reply_to", &root), ("body", "Reader reply")]).await.expect_status(StatusCode::SEE_OTHER);
    carol.post(&resolution, &[("resolved", "true")]).await.expect_status(StatusCode::FORBIDDEN);
    assert!(!carol.get(&files).await.contains("Resolve conversation"));
    bob.post_with(&resolution, &[("resolved", "true")], &[("Origin", "https://evil.example")]).await.expect_status(StatusCode::FORBIDDEN);
    bob.post(&resolution, &[("resolved", "invalid")]).await.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    bob.post(&resolution, &[("resolved", "true"), ("view", "files")]).await.expect_redirect(&format!("{files}#r{root}"));
    bob.post(&resolution, &[("resolved", "true")]).await.expect_redirect(&format!("{page}#r{root}"));
    for url in [page, &files] {
        let response = alice.get(url).await;
        response.expect_contains(&[
            "<details class=\"review-thread-resolved\">",
            "Resolved conversation",
            "Explain this",
            "Reader reply",
            "Reopen conversation",
        ]);
        let start = response.body.find("<details class=\"review-thread-resolved\">").unwrap();
        let end = start + response.body[start..].find("</details>").unwrap();
        assert!(response.body[start..end].contains("Reader reply"), "replies collapse with the root");
        assert!(!response.body[start..end].contains("<details class=\"review-thread-resolved\" open"));
    }
    let timeline = alice.get(page).await;
    assert_eq!(timeline.body.matches(&format!("resolved <a href=\"#r{root}\"")).count(), 1, "repeated posts do not duplicate events");
    alice.post(&resolution, &[("resolved", "false")]).await.expect_redirect(&format!("{page}#r{root}"));
    let reopened = bob.get(page).await;
    reopened.expect_contains(&["Resolve conversation", "reopened", "a review conversation"]);
    assert!(!reopened.contains("<details class=\"review-thread-resolved\">"));
    bob.post(
        &format!("{page}/comments"),
        &[("path", "feature.txt"), ("side", "right"), ("line", "2"), ("body", "Private draft"), ("action", "review")],
    )
    .await
    .expect_status(StatusCode::SEE_OTHER);
    let pending_files = bob.get(&files).await;
    let draft_at = pending_files.body.find("Private draft").unwrap();
    let draft_prefix = &pending_files.body[..draft_at];
    let pending_root = draft_prefix.rsplit_once("id=\"r").unwrap().1.split('"').next().unwrap();
    for actor in [&alice, &bob, &carol] {
        actor
            .post(&format!("{page}/comments/{pending_root}/resolution"), &[("resolved", "true")])
            .await
            .expect_status(StatusCode::NOT_FOUND);
    }
    assert!(!alice.get(&files).await.contains("Private draft"));
    git.ok(&sample.dir, &["push", "origin", "review-resolution:other"]);
    alice
        .post("/alice/demo/pulls", &[("base", "main"), ("head", "other"), ("title", "Another pull")])
        .await
        .expect_redirect("/alice/demo/pull/2");
    alice
        .post(&format!("/alice/demo/pull/2/comments/{root}/resolution"), &[("resolved", "true")])
        .await
        .expect_status(StatusCode::NOT_FOUND);
    // Hidden repositories stay hidden even when the comment id is known.
    alice.create_repo("alice", "secret", "private").await;
    carol
        .post(&format!("/alice/secret/pull/1/comments/{root}/resolution"), &[("resolved", "true")])
        .await
        .expect_status(StatusCode::NOT_FOUND);
}
