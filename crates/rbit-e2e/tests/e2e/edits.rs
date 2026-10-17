//! Editing files in the browser, end to end: the buttons only writers see, an edit that
//! `git clone` shows with the right author and that closes the issue it fixes, a branch
//! that moved meanwhile, new files, renames, deletions and uploads, a protected branch
//! taken through a new branch and a pull request, and the size limits. (Mirrors being
//! read-only is covered by the service tests: creating one here needs an upstream.)
//!
//! Milestone 3 workstream E (editing files in the browser) owns this file.

use std::{
    future::Future,
    time::{Duration, Instant},
};

use rbit_e2e::{Client, Git, PASSWORD, Response, StatusCode, TestServer, attribute_after};

/// Polls `check` until it holds, for at most 20 seconds.
async fn eventually<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = Instant::now() + Duration::from_secs(20);
    while !check().await {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The commit an editor page was opened at (its hidden `commit` field).
fn opened_at(page: &Response) -> String {
    page.expect_status(StatusCode::OK);
    attribute_after(&page.body, "name=\"commit\"", "value").expect("the form carries its commit")
}

/// The id `main` (or `branch`) points at on the server.
fn remote_tip(git: &Git, clone: &std::path::Path, branch: &str) -> String {
    git.ok(clone, &["fetch", "--quiet", "origin"]);
    git.ok(clone, &["rev-parse", &format!("origin/{branch}")]).trim().to_owned()
}

#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one scenario, told in order")]
async fn editing_a_file_in_the_browser() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "demo", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", PASSWORD));
    alice.post("/alice/demo/issues", &[("title", "The README is too short"), ("body", "")]).await.expect_redirect("/alice/demo/issues/1");

    // Only writers see the buttons and reach the pages; tags are not branches.
    alice
        .get("/alice/demo/blob/main/README.md")
        .await
        .expect_contains(&["href=\"/alice/demo/edit/main/README.md\"", "href=\"/alice/demo/delete/main/README.md\""]);
    alice
        .get("/alice/demo/tree/main/src")
        .await
        .expect_contains(&["href=\"/alice/demo/new/main/src\"", "href=\"/alice/demo/upload/main/src\""]);
    for page in [bob.get("/alice/demo/blob/main/README.md").await, alice.get("/alice/demo/blob/v1.0/README.md").await] {
        page.expect_status(StatusCode::OK);
        assert!(!page.contains("/edit/") && !page.contains("/delete/"), "no edit buttons");
    }
    assert!(!bob.get("/alice/demo").await.contains("/new/main"), "no Add file for readers");
    for path in ["/alice/demo/edit/main/README.md", "/alice/demo/new/main", "/alice/demo/upload/main", "/alice/demo/edit/v1.0/README.md"] {
        bob.get(path).await.expect_status(StatusCode::NOT_FOUND);
    }
    alice.get("/alice/demo/edit/v1.0/README.md").await.expect_status(StatusCode::NOT_FOUND);
    alice.get("/alice/demo/edit/main/nope.md").await.expect_status(StatusCode::NOT_FOUND);
    server.client().get("/alice/demo/edit/main/README.md").await.expect_status(StatusCode::SEE_OTHER);
    bob.post("/alice/demo/edit/main/README.md", &[("content", "x")]).await.expect_status(StatusCode::NOT_FOUND);

    // The editor shows the file; the commit lands on main, closing issue 1.
    let editor = alice.get("/alice/demo/edit/main/README.md").await;
    editor.expect_contains(&[
        "Edit <span class=\"font-mono text-lg\">README.md</span>",
        "Hello from the end-to-end suite.",
        "Commit directly to",
    ]);
    assert!(!editor.contains("style="), "no inline styles");
    let parent = opened_at(&editor);
    assert_eq!(parent, sample.head);
    let content = "# Sample project\r\n\r\nHello from the end-to-end suite.\r\nEdited in the browser.";
    let forged = alice
        .post_with("/alice/demo/edit/main/README.md", &[("commit", &parent), ("content", content)], &[("Origin", "https://evil.example")])
        .await;
    forged.expect_status(StatusCode::FORBIDDEN);
    let form = [
        ("commit", parent.as_str()),
        ("path", "README.md"),
        ("content", content),
        ("title", "Explain the edit"),
        ("description", "Fixes #1"),
        ("target", "branch"),
        ("new_branch", "alice-patch-1"),
    ];
    alice.post("/alice/demo/edit/main/README.md", &form).await.expect_redirect("/alice/demo/blob/main/README.md");
    alice.get("/alice/demo/blob/main/README.md").await.expect_contains(&["Edited in the browser."]);

    let clone = git.clone(&server.git_url("alice", "demo"));
    let log = git.ok(&clone, &["log", "-1", "--format=%an <%ae>|%cn <%ce>|%s|%b"]);
    assert_eq!(log.trim(), "alice <alice@noreply.127.0.0.1>|alice <alice@noreply.127.0.0.1>|Explain the edit|Fixes #1");
    assert_eq!(git.ok(&clone, &["rev-parse", "HEAD^"]).trim(), parent, "exactly one commit on top");
    let bytes = std::fs::read(clone.join("README.md")).unwrap();
    assert_eq!(bytes, b"# Sample project\n\nHello from the end-to-end suite.\nEdited in the browser.", "LF kept, nothing appended");
    eventually("issue 1 to close", || {
        let alice = &alice;
        async move { alice.get("/alice/demo/issues/1").await.contains("state-badge-closed") }
    })
    .await;

    // The old form is stale now: a clear conflict, nothing committed, the text kept.
    let stale = alice.post("/alice/demo/edit/main/README.md", &[("commit", &parent), ("path", "README.md"), ("content", "Mine")]).await;
    stale.expect_status(StatusCode::CONFLICT).expect_contains(&["main changed since you started", "Review the new commits", "Mine"]);
    let current = remote_tip(&git, &clone, "main");
    assert_eq!(attribute_after(&stale.body, "name=\"commit\"", "value"), Some(current.clone()), "the form now carries the new tip");
    assert_eq!(remote_tip(&git, &clone, "main"), current);

    // A new file in new directories, a rename, a delete and an upload: one commit each.
    let new = opened_at(&alice.get("/alice/demo/new/main/docs").await);
    let created =
        alice.post("/alice/demo/new/main/docs", &[("commit", &new), ("path", "docs/deep/notes.md"), ("content", "Notes\r\n")]).await;
    created.expect_redirect("/alice/demo/blob/main/docs/deep/notes.md");
    let invalid = opened_at(&alice.get("/alice/demo/new/main").await);
    let clobber = alice.post("/alice/demo/new/main", &[("commit", &invalid), ("path", "README.md"), ("content", "x")]).await;
    clobber.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["A file called README.md already exists."]);
    let escape = alice.post("/alice/demo/new/main", &[("commit", &invalid), ("path", "../x"), ("content", "x")]).await;
    escape.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["Paths cannot contain . or .. parts."]);

    let rename = opened_at(&alice.get("/alice/demo/edit/main/docs/guide.txt").await);
    let guide = "Plain text guide for the raw view.\r\n<script>alert(1)</script>\r\n";
    let moved = alice.post("/alice/demo/edit/main/docs/guide.txt", &[("commit", &rename), ("path", "guide.txt"), ("content", guide)]).await;
    moved.expect_redirect("/alice/demo/blob/main/guide.txt");

    let delete = alice.get("/alice/demo/delete/main/docs/deep/notes.md").await;
    delete.expect_contains(&["Delete <span class=\"font-mono text-lg\">docs/deep/notes.md</span>", "Delete file"]);
    let deleted = alice.post("/alice/demo/delete/main/docs/deep/notes.md", &[("commit", &opened_at(&delete))]).await;
    // docs/deep and docs are gone, so the root is shown.
    deleted.expect_redirect("/alice/demo/tree/main");

    let upload = alice.get("/alice/demo/upload/main/src").await;
    upload.expect_contains(&["Upload files", "enctype=\"multipart/form-data\""]);
    let files = [("files", "C:\\fakepath\\data.bin", vec![0_u8, 1, 2, 255]), ("files", "lib.rs", b"pub fn lib() {}\n".to_vec())];
    let uploaded = alice.post_multipart("/alice/demo/upload/main/src", &[("commit", &opened_at(&upload)), ("title", "")], &files).await;
    uploaded.expect_redirect("/alice/demo/tree/main/src");
    let uploaded_without_files =
        alice.post_multipart("/alice/demo/upload/main/src", &[("commit", &remote_tip(&git, &clone, "main"))], &[]).await;
    uploaded_without_files.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["Choose at least one file to upload."]);

    git.ok(&clone, &["pull", "--quiet", "--ff-only", "origin", "main"]);
    let subjects = git.ok(&clone, &["log", "-4", "--format=%s"]);
    assert_eq!(
        subjects.lines().collect::<Vec<_>>(),
        ["Add files via upload", "Delete docs/deep/notes.md", "Rename docs/guide.txt to guide.txt", "Create docs/deep/notes.md"]
    );
    assert_eq!(std::fs::read(clone.join("src/data.bin")).unwrap(), [0, 1, 2, 255]);
    assert_eq!(std::fs::read(clone.join("guide.txt")).unwrap(), rbit_e2e::Sample::GUIDE.as_bytes());
    assert!(!clone.join("docs").exists());
}

#[tokio::test]
async fn a_protected_branch_takes_a_new_branch_and_a_pull_request() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "demo", "public").await;
    alice
        .post("/alice/demo/settings/collaborators", &[("username", "bob"), ("permission", "write")])
        .await
        .expect_status(StatusCode::SEE_OTHER);
    git.push_sample(&git.sample(), &server.git_url_as("alice", "demo", "alice", PASSWORD));
    let rule = [
        ("pattern", "main"),
        ("deny_force_push", "on"),
        ("deny_deletion", "on"),
        ("require_pull_request", "on"),
        ("required_approvals", "0"),
    ];
    alice.post("/alice/demo/settings/branches", &rule).await.expect_status(StatusCode::SEE_OTHER);

    // The form offers only the new branch.
    let editor = bob.get("/alice/demo/edit/main/README.md").await;
    editor.expect_contains(&["Not allowed: protected branch: changes must go through a pull request.", "value=\"bob-patch-1\""]);
    let parent = opened_at(&editor);
    let direct = [("commit", parent.as_str()), ("path", "README.md"), ("content", "# Bob was here\n"), ("target", "branch")];
    let refused = bob.post("/alice/demo/edit/main/README.md", &direct).await;
    refused.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["You cannot commit to main directly", "# Bob was here"]);

    let via_branch = [
        ("commit", parent.as_str()),
        ("path", "README.md"),
        ("content", "# Bob was here\n"),
        ("target", "new"),
        ("new_branch", "bob-patch-1"),
    ];
    let created = bob.post("/alice/demo/edit/main/README.md", &via_branch).await;
    created.expect_redirect("/alice/demo/pulls/new?base=main&head=bob%2Dpatch%2D1");
    bob.get(created.location().unwrap()).await.expect_status(StatusCode::OK).expect_contains(&["Update README.md"]);
    let clone = git.clone(&server.git_url("alice", "demo"));
    assert_eq!(remote_tip(&git, &clone, "main"), parent, "main did not move");
    let commit = git.ok(&clone, &["log", "-1", "--format=%an|%s", "origin/bob-patch-1"]);
    assert_eq!(commit.trim(), "bob|Update README.md");
    assert_eq!(git.ok(&clone, &["rev-parse", "origin/bob-patch-1^"]).trim(), parent);

    // The suggestion moves on; a taken name is refused.
    bob.get("/alice/demo/edit/main/README.md").await.expect_contains(&["value=\"bob-patch-2\""]);
    let taken = bob.post("/alice/demo/edit/main/README.md", &via_branch).await;
    taken.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["A branch called bob-patch-1 already exists."]);

    // The owner is held to the rule too (no bypass configured).
    alice.get("/alice/demo/new/main").await.expect_contains(&["Not allowed: protected branch"]);
}

#[tokio::test]
async fn large_text_fits_the_editor_and_binaries_do_not() {
    let server = TestServer::start();
    let git = Git::new();
    let alice: Client = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    let sample = git.sample();
    std::fs::write(sample.dir.join("logo.bin"), [0_u8, 159, 146, 150]).unwrap();
    git.ok(&sample.dir, &["add", "--all"]);
    git.ok(&sample.dir, &["commit", "--quiet", "-m", "Add a binary"]);
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", PASSWORD));

    let blob = alice.get("/alice/demo/blob/main/logo.bin").await;
    assert!(!blob.contains("/edit/main/logo.bin") && blob.contains("/delete/main/logo.bin"), "delete, but no editor");
    alice.get("/alice/demo/edit/main/logo.bin").await.expect_contains(&["cannot be edited in the browser"]);

    // Close to 1 MiB of short lines is well over the page router's 1 MiB form limit once
    // encoded; the editor's own limit takes it.
    let parent = opened_at(&alice.get("/alice/demo/new/main").await);
    let text = "ab\r\n".repeat(200_000);
    alice
        .post("/alice/demo/new/main", &[("commit", &parent), ("path", "big.txt"), ("content", &text)])
        .await
        .expect_redirect("/alice/demo/blob/main/big.txt");
    let too_big = "x".repeat(1024 * 1024 + 1);
    let parent = opened_at(&alice.get("/alice/demo/new/main").await);
    let refused = alice.post("/alice/demo/new/main", &[("commit", &parent), ("path", "huge.txt"), ("content", &too_big)]).await;
    refused.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["Files edited in the browser can be at most 1 MiB."]);
}
