//! Importing from a fake GitHub through the pages, then mirroring the imported repository
//! to a push mirror: the form, the progress page, the imported history over HTTP and Git,
//! and a push that reaches the mirror (with prune) — all against a real server.
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file. The fake forge is
//! shared with the services tests.

#[path = "../../../rbit-services/tests/support/fake_forge.rs"]
mod fake_forge;

use std::{path::Path, time::Duration};

use fake_forge::FakeForge;
use rbit_e2e::{Client, Git, StatusCode, TestServer};

const TOKEN: &str = "ghp_e2e_token";

/// Polls `path` until its body contains `needle`.
async fn wait_for_page(client: &Client, path: &str, needle: &str) -> String {
    for _ in 0..300 {
        let page = client.get(path).await;
        page.expect_status(StatusCode::OK);
        if page.contains(needle) {
            return page.body;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("{path} never showed {needle:?}");
}

/// Waits until `check` holds for the bare repository at `path`.
async fn wait_for_refs(git: &Git, path: &Path, check: impl Fn(&str) -> bool) -> String {
    for _ in 0..300 {
        let refs = git.ok(path, &["for-each-ref", "--format=%(refname) %(objectname)"]);
        if check(&refs) {
            return refs;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the mirror never got there: {}", git.ok(path, &["for-each-ref"]));
}

#[tokio::test]
async fn import_from_github_then_push_to_a_mirror() {
    let forge = FakeForge::start(TOKEN, None).await;
    let server = TestServer::start_with(|config| {
        config.imports.allow_private_addresses = true;
        config.push_mirrors.allow_local_urls = true;
        config.push_mirrors.debounce_seconds = 0;
    });
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;

    // The form is offered from "New repository".
    alice.get("/new").await.expect_contains(&["href=\"/new/import\""]);
    alice.get("/new/import").await.expect_status(StatusCode::OK).expect_contains(&["Import a repository", "GitHub Enterprise", "Wiki"]);
    let refused = alice
        .post(
            "/new/import",
            &[("kind", "github_enterprise"), ("host", &forge.base), ("repository", "acme/rocket"), ("token", "wrong"), ("owner", "alice")],
        )
        .await;
    refused.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["token"]);
    assert!(!refused.contains("value=\"wrong\""), "the token is never sent back");

    let form = [
        ("kind", "github_enterprise"),
        ("host", forge.base.as_str()),
        ("repository", "acme/rocket"),
        ("token", TOKEN),
        ("owner", "alice"),
        ("name", ""),
        ("visibility", "public"),
        ("issues", "on"),
        ("labels", "on"),
        ("milestones", "on"),
        ("releases", "on"),
        ("lfs", "on"),
    ];
    alice.post("/new/import", &form).await.expect_redirect("/alice/rocket/import");
    let progress = wait_for_page(&alice, "/alice/rocket/import", "Go to the repository").await;
    for needle in ["Finished", "3 issues, 4 pull requests, 4 comments", "3 labels", "2 releases, 1 file", "1 object"] {
        assert!(progress.contains(needle), "{needle:?} in the progress page");
    }
    assert!(!progress.contains(TOKEN));
    bob.get("/alice/rocket/import").await.expect_status(StatusCode::NOT_FOUND);
    server.client().get("/alice/rocket/import").await.expect_status(StatusCode::NOT_FOUND);

    // The history is there, numbered as on the source and attributed to its authors.
    alice.get("/alice/rocket/issues/3").await.expect_status(StatusCode::OK).expect_contains(&[
        "It crashes",
        "Originally by",
        "@octocat",
        "Same here",
    ]);
    alice.get("/alice/rocket/pull/2").await.expect_status(StatusCode::OK).expect_contains(&["Add a feature", "Draft", "feature"]);
    alice.get("/alice/rocket/pull/1").await.expect_status(StatusCode::OK).expect_contains(&["Merged"]);
    alice.get("/alice/rocket/releases/tag/v1.0").await.expect_status(StatusCode::OK).expect_contains(&["rocket.tar.gz"]);
    let git = Git::new();
    let clone = git.clone(&server.git_url("alice", "rocket"));
    assert!(clone.join("README.md").exists());
    let remote_refs = git.ok(&clone, &["ls-remote", "origin"]);
    assert!(remote_refs.contains("refs/pull/2/head") && remote_refs.contains("refs/tags/v1.0"), "{remote_refs}");

    // A push mirror receives everything at once, then follows pushes (with prune).
    let mirror = git.fresh_path("mirror");
    std::fs::create_dir_all(&mirror).unwrap();
    git.ok(&mirror, &["init", "--quiet", "--bare"]);
    let mirror_url = format!("file:///{}", mirror.to_string_lossy().replace('\\', "/").trim_start_matches('/'));
    alice.get("/alice/rocket/settings").await.expect_contains(&["/alice/rocket/settings/push-mirrors"]);
    bob.get("/alice/rocket/settings/push-mirrors").await.expect_status(StatusCode::NOT_FOUND);
    alice
        .post(
            "/alice/rocket/settings/push-mirrors",
            &[("url", &mirror_url), ("username", "bot"), ("secret", "hunter2"), ("sync_on_push", "on")],
        )
        .await
        .expect_redirect("/alice/rocket/settings/push-mirrors?added=1");
    // The mirror's refs are updated one by one, so wait for all of them.
    let refs = wait_for_refs(&git, &mirror, |refs| {
        refs.contains("refs/heads/main") && refs.contains("refs/heads/feature") && refs.contains("refs/tags/v1.0")
    })
    .await;
    assert!(!refs.contains("refs/pull/"), "{refs}");
    let settings = wait_for_page(&alice, "/alice/rocket/settings/push-mirrors", "Last push").await;
    assert!(settings.contains("secret configured") && !settings.contains("hunter2"), "the secret is write-only");

    let token = alice.create_token("push", "write").await;
    let pushed = git.commit_file(&clone, "NEWS.md", "Imported!\n");
    let url = server.git_url_as("alice", "rocket", "alice", &token);
    git.ok(&clone, &["push", "--quiet", &url, "HEAD:main", ":feature"]);
    let refs = wait_for_refs(&git, &mirror, |refs| refs.contains(&pushed)).await;
    assert!(!refs.contains("refs/heads/feature"), "deleted branches are pruned: {refs}");
    wait_for_page(&alice, "/alice/rocket/settings/push-mirrors", "text-success\">OK").await;
}
