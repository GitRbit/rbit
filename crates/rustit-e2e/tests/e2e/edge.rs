//! Edge cases in names, paths and Git objects that real repositories contain.

use std::path::Path;

use rustit_e2e::{Git, PASSWORD, StatusCode, TestServer};

/// Commits a gitlink (submodule entry) and a symlink without touching the work tree.
fn commit_gitlink_and_symlink(git: &Git, dir: &Path) {
    let root = git.ok(dir, &["rev-list", "--max-parents=0", "HEAD"]);
    git.ok(dir, &["update-index", "--add", "--cacheinfo", &format!("160000,{},vendor/sub", root.trim())]);
    std::fs::write(dir.join("link-target"), "src/main.rs").expect("write the link target");
    let blob = git.ok(dir, &["hash-object", "-w", "link-target"]);
    std::fs::remove_file(dir.join("link-target")).expect("remove the link target");
    git.ok(dir, &["update-index", "--add", "--cacheinfo", &format!("120000,{},link", blob.trim())]);
    git.ok(dir, &["commit", "--quiet", "-m", "Add a submodule entry and a symlink"]);
}

#[tokio::test]
async fn unusual_file_names_and_ref_names_resolve() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    let sample = git.sample();
    let files = [
        ("docs/hello world.txt", "hello%20world.txt", "spaces\n"),
        ("docs/100%.txt", "100%25.txt", "percent\n"),
        ("docs/#hash.txt", "%23hash.txt", "hash\n"),
        ("docs/ünïcödé.txt", "%C3%BCn%C3%AFc%C3%B6d%C3%A9.txt", "unicode\n"),
    ];
    for (name, _, contents) in files {
        git.commit_file(&sample.dir, name, contents);
    }
    git.commit_file(&sample.dir, "data.bin", "binary\0data");
    git.ok(&sample.dir, &["branch", "release/1.x"]);
    git.ok(&sample.dir, &["push", "--quiet", &server.git_url_as("alice", "demo", "alice", PASSWORD), "main", "release/1.x"]);

    let web = server.client();
    let tree = web.get("/alice/demo/tree/main/docs").await;
    tree.expect_status(StatusCode::OK);
    for (_, encoded, contents) in files {
        assert!(tree.contains(&format!("/alice/demo/blob/main/docs/{encoded}\"")), "the tree links {encoded}");
        web.get(&format!("/alice/demo/blob/main/docs/{encoded}")).await.expect_status(StatusCode::OK);
        let raw = web.get(&format!("/alice/demo/raw/main/docs/{encoded}")).await;
        raw.expect_status(StatusCode::OK);
        assert_eq!(raw.body, contents);
    }
    let binary = web.get("/alice/demo/raw/main/data.bin").await;
    binary.expect_status(StatusCode::OK);
    assert_eq!(binary.header("content-type"), Some("application/octet-stream"));
    assert_eq!(binary.header("content-security-policy"), Some("sandbox; default-src 'none'"));

    // Branch names with slashes resolve in every view.
    for path in ["/alice/demo/tree/release/1.x/docs", "/alice/demo/blob/release/1.x/README.md", "/alice/demo/commits/release/1.x"] {
        web.get(path).await.expect_status(StatusCode::OK);
    }
    web.get("/alice/demo/raw/release/1.x/docs/%23hash.txt").await.expect_status(StatusCode::OK);

    // Traversal attempts never leave the repository.
    for path in [
        "/alice/demo/tree/main/..",
        "/alice/demo/blob/main/../../README.md",
        "/alice/demo/raw/main/%2e%2e/README.md",
        "/alice/demo/raw/main/docs/%2e%2e/%2e%2e/README.md",
    ] {
        web.get(path).await.expect_status(StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn unusual_git_objects_render() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    let sample = git.sample();
    let dir = &sample.dir;
    commit_gitlink_and_symlink(&git, dir);
    git.ok(dir, &["tag", "blob-tag", "HEAD:README.md"]);
    git.ok(dir, &["tag", "-a", "-m", "A tag on a tree", "tree-tag", "HEAD^{tree}"]);
    git.ok(dir, &["merge", "--quiet", "--no-ff", "-m", "Merge the feature", "feature"]);
    git.ok(dir, &["-c", "i18n.commitEncoding=ISO-8859-1", "commit", "--quiet", "--allow-empty", "-m", "Caf\u{e9} message"]);
    let merge = git.ok(dir, &["rev-parse", "HEAD~1"]);
    let root = git.ok(dir, &["rev-list", "--max-parents=0", "HEAD"]);
    git.ok(dir, &["push", "--quiet", &server.git_url_as("alice", "demo", "alice", PASSWORD), "main", "blob-tag", "tree-tag", "v1.0"]);

    let web = server.client();
    web.get("/alice/demo").await.expect_status(StatusCode::OK).expect_contains(&["vendor", "link"]);
    web.get("/alice/demo/tree/main/vendor").await.expect_status(StatusCode::OK).expect_contains(&["sub"]);
    web.get("/alice/demo/blob/main/link").await.expect_status(StatusCode::OK);
    // Tags on blobs and trees have no commit to show; they must not break the page.
    web.get("/alice/demo/tags").await.expect_status(StatusCode::OK).expect_contains(&[">v1.0<"]);
    web.get(&format!("/alice/demo/commit/{}", merge.trim())).await.expect_status(StatusCode::OK).expect_contains(&["Merge the feature"]);
    web.get(&format!("/alice/demo/commit/{}", root.trim())).await.expect_status(StatusCode::OK).expect_contains(&[
        "Initial commit",
        "README.md",
        "@@ -0,0 +1,3 @@",
    ]);
    web.get("/alice/demo/commits").await.expect_status(StatusCode::OK).expect_contains(&["Caf", "Merge the feature"]);
    web.get("/alice/demo/commits?page=2").await.expect_status(StatusCode::OK);
}

#[tokio::test]
async fn names_are_case_insensitive_and_shared_by_users_and_organisations() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("Alice", "Demo", "ALICE", PASSWORD));
    server.client().sign_in("ALICE", PASSWORD).await.expect_redirect("/");
    server.client().get("/ALICE/DEMO/blob/main/README.md").await.expect_status(StatusCode::OK);
    let clone = git.clone(&server.git_url("ALICE", "DEMO"));
    assert_eq!(git.ok(&clone, &["rev-parse", "HEAD"]).trim(), sample.head);

    // A case-only rename is a rename, not a conflict with itself.
    alice.post("/alice/demo/settings/rename", &[("name", "Demo")]).await.expect_redirect("/alice/Demo/settings");
    alice.get("/alice/demo").await.expect_status(StatusCode::OK).expect_contains(&["alice/Demo"]);

    // Users and organisations share one namespace, whatever the case.
    alice.create_org("acme").await;
    server.client().sign_up("ACME").await.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    alice
        .post("/organizations/new", &[("name", "Alice"), ("display_name", ""), ("description", "")])
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn default_branch_setting_moves_head_for_clones() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", PASSWORD));

    alice
        .post("/alice/demo/settings", &[("description", ""), ("visibility", "public"), ("default_branch", "feature")])
        .await
        .expect_redirect("/alice/demo/settings?saved=1");
    let clone = git.clone(&server.git_url("alice", "demo"));
    assert_eq!(git.ok(&clone, &["rev-parse", "--abbrev-ref", "HEAD"]).trim(), "feature");
    server.client().get("/alice/demo").await.expect_status(StatusCode::OK).expect_contains(&["feature.txt"]);

    // A branch that does not exist is refused.
    alice
        .post("/alice/demo/settings", &[("description", ""), ("visibility", "public"), ("default_branch", "nope")])
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_recreated_repository_starts_empty() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", PASSWORD));
    alice.post("/alice/demo/star", &[("action", "star")]).await.expect_redirect("/alice/demo");

    alice.post("/alice/demo/settings/delete", &[("confirm", "alice/demo")]).await.expect_redirect("/alice");
    alice.create_repo("alice", "demo", "public").await;
    let home = alice.get("/alice/demo").await;
    home.expect_status(StatusCode::OK).expect_contains(&["aria-label=\"0 stargazers\""]);
    assert!(!home.contains("Sample project"), "no content survives deletion");
    assert_eq!(git.ok(&sample.dir, &["ls-remote", &server.git_url("alice", "demo")]), "");
}
