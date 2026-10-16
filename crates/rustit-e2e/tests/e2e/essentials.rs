//! Archives, releases, blame, compare, forks and README links, end to end.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.

use rustit_e2e::{Git, PASSWORD, Sample, StatusCode, TestServer};

#[tokio::test]
async fn readme_links_resolve_into_the_repository() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "docs", "public").await;
    let sample = git.sample();
    std::fs::write(sample.dir.join("logo.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    git.commit_file(&sample.dir, "README.md", "# Docs\n\n[Guide](docs/guide.txt) [Up](../../x) ![Logo](logo.png)\n");
    git.push_sample(&sample, &server.git_url_as("alice", "docs", "alice", PASSWORD));

    let home = server.client().get("/alice/docs").await;
    home.expect_status(StatusCode::OK).expect_contains(&[
        r#"href="/alice/docs/blob/main/docs/guide.txt""#,
        r#"src="/alice/docs/raw/main/logo.png""#,
        r#"<a id="docs""#,
        "<h1>Docs</h1>",
    ]);
    assert!(!home.contains("../../x"), "links escaping the repository are dropped");

    let image = server.client().get("/alice/docs/raw/main/logo.png").await;
    image.expect_status(StatusCode::OK);
    assert_eq!(image.header("content-type"), Some("image/png"));
    // A directory named as a file redirects to its tree.
    let directory = server.client().get("/alice/docs/blob/main/docs").await;
    assert_eq!((directory.status, directory.location()), (StatusCode::SEE_OTHER, Some("/alice/docs/tree/main/docs")));
}

#[tokio::test]
async fn archives_of_private_repositories_need_access() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "secret", "private").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "secret", "alice", PASSWORD));

    for path in ["/alice/secret/archive/main.zip", "/alice/secret/archive/v1.0.tar.gz"] {
        server.client().get(path).await.expect_status(StatusCode::NOT_FOUND);
        bob.get(path).await.expect_status(StatusCode::NOT_FOUND);
    }

    let zip = alice.get("/alice/secret/archive/main.zip").await;
    zip.expect_status(StatusCode::OK);
    assert_eq!(zip.header("content-type"), Some("application/zip"));
    assert_eq!(zip.header("content-disposition"), Some("attachment; filename=\"secret-main.zip\""));
    assert_eq!(zip.header("etag"), Some(format!("\"{}\"", sample.head).as_str()));
    assert!(zip.body.starts_with("PK"), "a zip file");

    let tarball = alice.get("/alice/secret/archive/feature.tar.gz").await;
    tarball.expect_status(StatusCode::OK);
    assert_eq!(tarball.header("content-type"), Some("application/gzip"));
    let etag = format!("\"{}\"", sample.head);
    let cached = alice.get_with("/alice/secret/archive/main.zip", &[("If-None-Match", &etag)]).await;
    cached.expect_status(StatusCode::NOT_MODIFIED);
    alice.get("/alice/secret/archive/no-such-branch.zip").await.expect_status(StatusCode::NOT_FOUND);
    alice.get("/alice/secret/archive/main.rar").await.expect_status(StatusCode::NOT_FOUND);

    // The clone menu offers the default branch as a zip.
    alice.get("/alice/secret").await.expect_contains(&["href=\"/alice/secret/archive/main.zip\""]);
}

#[tokio::test]
async fn a_published_release_appears_on_the_releases_tab() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "app", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "app", "alice", PASSWORD));
    let anonymous = server.client();

    // Only writers get the editor, and a release needs an existing tag.
    anonymous.get("/alice/app/releases/new").await.expect_status(StatusCode::SEE_OTHER);
    let bob = server.sign_up("bob").await;
    bob.get("/alice/app/releases/new").await.expect_status(StatusCode::NOT_FOUND);
    let missing_tag = alice.post("/alice/app/releases/new", &[("tag", "v9"), ("name", ""), ("notes", ""), ("action", "publish")]).await;
    missing_tag.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["There is no tag called v9"]);

    // A draft is invisible to readers.
    alice
        .post(
            "/alice/app/releases/new",
            &[("tag", Sample::TAG), ("name", "First"), ("notes", "See the [guide](docs/guide.txt)."), ("action", "draft")],
        )
        .await
        .expect_redirect("/alice/app/releases/tag/v1.0");
    anonymous.get("/alice/app/releases/tag/v1.0").await.expect_status(StatusCode::NOT_FOUND);
    assert!(!anonymous.get("/alice/app/releases").await.contains("First"));
    alice.get("/alice/app/releases").await.expect_contains(&["First", "Draft"]);

    // Publishing it shows it to everyone, with notes linking into the tag and archives.
    alice.get("/alice/app/releases").await.expect_contains(&["href=\"/alice/app/releases/1/edit\""]);
    alice
        .post(
            "/alice/app/releases/1/edit",
            &[("tag", Sample::TAG), ("name", "First"), ("notes", "See the [guide](docs/guide.txt)."), ("action", "publish")],
        )
        .await
        .expect_redirect("/alice/app/releases/tag/v1.0");
    let page = anonymous.get("/alice/app/releases").await;
    page.expect_status(StatusCode::OK).expect_contains(&[
        "First",
        "Latest",
        r#"href="/alice/app/blob/v1.0/docs/guide.txt""#,
        r#"href="/alice/app/archive/v1.0.zip""#,
        r#"href="/alice/app/archive/v1.0.tar.gz""#,
    ]);
    anonymous.get("/alice/app").await.expect_contains(&["href=\"/alice/app/releases\"", "First"]);
    anonymous.get("/alice/app/releases/download/v1.0/nothing.zip").await.expect_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn blame_and_compare_pages_render() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "code", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "code", "alice", PASSWORD));
    let anonymous = server.client();

    let blame = anonymous.get("/alice/code/blame/main/src/main.rs").await;
    blame.expect_status(StatusCode::OK).expect_contains(&[Sample::SECOND_MESSAGE, "id=\"L2\""]);
    anonymous.get("/alice/code/blob/main/src/main.rs").await.expect_contains(&["href=\"/alice/code/blame/main/src/main.rs\""]);
    anonymous.get("/alice/code/blame/main/nope.rs").await.expect_status(StatusCode::NOT_FOUND);

    let compare = anonymous.get("/alice/code/compare/main...feature").await;
    compare.expect_status(StatusCode::OK).expect_contains(&["1 ahead", "0 behind", "Feature work", "feature.txt"]);
    anonymous.get("/alice/code/compare?base=main&head=feature").await.expect_redirect("/alice/code/compare/main...feature");
    anonymous.get("/alice/code/compare/main...nope").await.expect_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn forks_can_be_pushed_to_and_keep_private_parents_private() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "lib", "public").await;
    alice.create_repo("alice", "hidden", "private").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "lib", "alice", PASSWORD));

    bob.post("/alice/lib/fork", &[("owner", "bob"), ("name", "")]).await.expect_redirect("/bob/lib");
    let fork = bob.get("/bob/lib").await;
    fork.expect_status(StatusCode::OK).expect_contains(&["forked from", "href=\"/alice/lib\""]);
    server.client().get("/alice/lib/forks").await.expect_contains(&["bob/", "aria-label=\"1 fork\""]);

    // The fork has the parent's history and accepts bob's pushes.
    let clone = git.clone(&server.git_url_as("bob", "lib", "bob", PASSWORD));
    assert_eq!(git.ok(&clone, &["rev-parse", "HEAD"]).trim(), sample.head);
    let pushed = git.commit_file(&clone, "bob.txt", "from bob\n");
    git.ok(&clone, &["push", "--quiet", "origin", "main"]);
    let parent = git.clone(&server.git_url("alice", "lib"));
    assert_eq!(git.ok(&parent, &["rev-parse", "HEAD"]).trim(), sample.head, "the parent is untouched");
    assert_ne!(pushed, sample.head);

    // Private repositories cannot be forked by outsiders, and forks of them stay private.
    bob.post("/alice/hidden/fork", &[("owner", "bob"), ("name", "")]).await.expect_status(StatusCode::NOT_FOUND);
    alice.post("/alice/hidden/fork", &[("owner", "alice"), ("name", "hidden-copy")]).await.expect_redirect("/alice/hidden-copy");
    bob.get("/alice/hidden-copy").await.expect_status(StatusCode::NOT_FOUND);
}
