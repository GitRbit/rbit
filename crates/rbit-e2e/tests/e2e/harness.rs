//! The harness itself: servers boot, are isolated from each other, and clean up.

use rbit_e2e::{Git, StatusCode, TestServer};

#[tokio::test]
async fn servers_boot_on_distinct_ports_and_clean_up() {
    let first = TestServer::start();
    let second = TestServer::start();
    assert_ne!(first.addr(), second.addr());

    first.client().get("/").await.expect_status(StatusCode::OK);
    second.client().get("/healthz").await.expect_status(StatusCode::OK);

    let data_dir = first.data_dir().to_path_buf();
    assert!(data_dir.join("rbit.db").exists());
    drop(first);
    assert!(!data_dir.exists(), "the data directory is removed once the server stops");
}

#[test]
fn git_runs_with_an_isolated_configuration() {
    let git = Git::new();
    let sample = git.sample();
    let author = git.ok(&sample.dir, &["log", "--format=%an <%ae>", "-1"]);
    assert_eq!(author.trim(), "E2E Tester <e2e@example.test>");
    let helpers = git.run(&sample.dir, &["config", "--get-all", "credential.helper"]);
    assert_eq!(helpers.stdout.trim(), "", "no credential helper is configured");
    let branches = git.ok(&sample.dir, &["branch", "--format=%(refname:short)"]);
    assert_eq!(branches.lines().collect::<Vec<_>>(), ["feature", "main"]);
}
