//! Repositories: create, push over HTTP, browse every view, rename and delete.

use rbit_e2e::{Git, PASSWORD, Sample, StatusCode, TestServer};

#[tokio::test]
async fn push_then_browse_every_view() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;

    // An empty repository explains how to push.
    alice.get("/alice/demo").await.expect_status(StatusCode::OK).expect_contains(&[&server.git_url("alice", "demo")]);

    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", PASSWORD));
    let web = server.client();

    // Home: file list and the rendered README.
    web.get("/alice/demo").await.expect_status(StatusCode::OK).expect_contains(&[
        "README.md",
        "src",
        "docs",
        "<h1>Sample project</h1>",
        "Hello from the end-to-end suite.",
    ]);

    // Trees, on a branch, a tag and a nested directory.
    web.get("/alice/demo/tree/main/src").await.expect_status(StatusCode::OK).expect_contains(&["main.rs"]);
    let feature = web.get("/alice/demo/tree/feature").await;
    feature.expect_status(StatusCode::OK).expect_contains(&["feature.txt"]);
    let tagged = web.get("/alice/demo/tree/v1.0").await;
    tagged.expect_status(StatusCode::OK).expect_contains(&["docs"]);
    assert!(!tagged.contains("feature.txt"), "the tag predates the feature branch");
    web.get("/alice/demo/tree/main/missing").await.expect_status(StatusCode::NOT_FOUND);

    // Blob: content is shown, escaped.
    let blob = web.get("/alice/demo/blob/main/docs/guide.txt").await;
    blob.expect_status(StatusCode::OK).expect_contains(&["Plain text guide for the raw view.", "&lt;script&gt;"]);
    assert!(!blob.contains("<script>alert(1)</script>"), "file content is never inserted unescaped");

    // Raw: the exact bytes, as inert plain text.
    let raw = web.get("/alice/demo/raw/main/docs/guide.txt").await;
    raw.expect_status(StatusCode::OK);
    assert_eq!(raw.body, Sample::GUIDE);
    assert_eq!(raw.header("content-type"), Some("text/plain; charset=utf-8"));
    assert_eq!(raw.header("content-security-policy"), Some("sandbox; default-src 'none'"));
    assert_eq!(raw.header("x-content-type-options"), Some("nosniff"));
    let first = git.ok(&sample.dir, &["rev-parse", "main~1"]);
    let old_main = web.get(&format!("/alice/demo/raw/{}/src/main.rs", first.trim())).await;
    old_main.expect_status(StatusCode::OK);
    assert!(old_main.body.contains("println!(\"hello\");"), "commit ids resolve in raw paths");

    // History.
    web.get("/alice/demo/commits").await.expect_status(StatusCode::OK).expect_contains(&[Sample::FIRST_MESSAGE, Sample::SECOND_MESSAGE]);
    let feature_log = web.get("/alice/demo/commits/feature").await;
    feature_log.expect_status(StatusCode::OK).expect_contains(&["Feature work", Sample::FIRST_MESSAGE]);

    // A commit with its diff: hunk headers as Git writes them.
    web.get(&format!("/alice/demo/commit/{}", sample.head)).await.expect_status(StatusCode::OK).expect_contains(&[
        Sample::SECOND_MESSAGE,
        "src/main.rs",
        "docs/guide.txt",
        "@@ -1,3 +1,3 @@",
        "@@ -0,0 +1,2 @@",
        "hello, rbit",
    ]);
    web.get("/alice/demo/commit/0000000000000000000000000000000000000000").await.expect_status(StatusCode::NOT_FOUND);

    // Branches and tags.
    web.get("/alice/demo/branches").await.expect_status(StatusCode::OK).expect_contains(&[">main<", ">feature<"]);
    web.get("/alice/demo/branches?view=tree").await.expect_status(StatusCode::OK).expect_contains(&[
        "Branch commit graph",
        "Feature work",
        Sample::FIRST_MESSAGE,
        Sample::SECOND_MESSAGE,
        "aria-current=\"page\">",
    ]);
    let language_files = web.get("/alice/demo/language-files?revision=main&language=Rust").await;
    language_files.expect_status(StatusCode::OK).expect_contains(&["Rust files", "src/main.rs", "data-outline-folder"]);
    assert!(!language_files.contains("guide.txt"), "language browser includes only matching files");
    web.get("/alice/demo/tags").await.expect_status(StatusCode::OK).expect_contains(&[">v1.0<"]);

    // Anonymous clone of a public repository returns the same history.
    let clone = git.clone(&server.git_url("alice", "demo"));
    assert_eq!(git.ok(&clone, &["rev-parse", "HEAD"]).trim(), sample.head);
    assert_eq!(std::fs::read_to_string(clone.join("docs").join("guide.txt")).expect("cloned file"), Sample::GUIDE);
    let remote_branches = git.ok(&clone, &["branch", "--remotes", "--format=%(refname:short)"]);
    assert!(remote_branches.contains("origin/feature"), "every pushed branch is cloned: {remote_branches}");
    assert_eq!(git.ok(&clone, &["tag", "--list"]).trim(), Sample::TAG);

    // Anonymous visitors may read but are asked for credentials to push.
    let challenge = web.get("/alice/demo.git/info/refs?service=git-receive-pack").await;
    challenge.expect_status(StatusCode::UNAUTHORIZED);
    assert!(challenge.header("www-authenticate").is_some_and(|v| v.starts_with("Basic ")));
    let refused = git.fails(&clone, &["push", &server.git_url("alice", "demo"), "HEAD:refs/heads/anonymous"]);
    assert!(refused.contains("could not read Username"), "git was challenged for credentials: {refused}");
}

#[tokio::test]
async fn renamed_repository_clones_at_its_new_url_and_deleted_one_is_gone() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "before", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "before", "alice", PASSWORD));

    alice.post("/alice/before/settings/rename", &[("name", "after")]).await.expect_redirect("/alice/after/settings");
    alice.get("/alice/after").await.expect_status(StatusCode::OK).expect_contains(&["Sample project"]);
    // The old name redirects (see `redirects.rs` for the details).
    alice.get("/alice/before").await.expect_status(StatusCode::MOVED_PERMANENTLY);
    let clone = git.clone(&server.git_url("alice", "after"));
    assert_eq!(git.ok(&clone, &["rev-parse", "HEAD"]).trim(), sample.head);
    git.ok(&clone, &["push", "--quiet", &server.git_url_as("alice", "after", "alice", PASSWORD), "HEAD:refs/heads/renamed"]);

    // The old name is free again, and the new repository wins over the redirect.
    alice.create_repo("alice", "before", "public").await;
    alice.get("/alice/before").await.expect_status(StatusCode::OK);

    // Deleting requires typing the full name.
    alice.post("/alice/after/settings/delete", &[("confirm", "after")]).await.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    alice.post("/alice/after/settings/delete", &[("confirm", "alice/after")]).await.expect_redirect("/alice");
    alice.get("/alice/after").await.expect_status(StatusCode::NOT_FOUND);
    server.client().get("/alice/after").await.expect_status(StatusCode::NOT_FOUND);
    alice.get_basic("/alice/after.git/info/refs?service=git-upload-pack", "alice", PASSWORD).await.expect_status(StatusCode::NOT_FOUND);
    git.clone_fails(&server.git_url_as("alice", "after", "alice", PASSWORD));
}

#[tokio::test]
async fn invalid_and_duplicate_names_are_rejected() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;

    for name in ["demo", "DEMO", "../escape", ".hidden", "repo.git", "a/b", "settings"] {
        let page = alice.post("/new", &[("owner", "alice"), ("name", name), ("visibility", "public")]).await;
        page.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    }
    // Nobody can create repositories under someone else's name.
    let bob = server.sign_up("bob").await;
    let page = bob.post("/new", &[("owner", "alice"), ("name", "intruder"), ("visibility", "public")]).await;
    page.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["cannot create repositories for alice"]);
    bob.get("/alice/intruder").await.expect_status(StatusCode::NOT_FOUND);
}
