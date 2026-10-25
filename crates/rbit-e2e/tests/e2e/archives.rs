//! Archived repositories, end to end: archiving from the settings page, pushes refused
//! over smart HTTP and SSH with the real `git` CLI (and OpenSSH), merges and new issues
//! refused, reads, clones, forks and stars still working, and unarchiving restoring
//! writes. Skips the SSH half (with a message) when OpenSSH is not installed.

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use rbit_core::{Config, Visibility, repository::ARCHIVED_READ_ONLY};
use rbit_e2e::{Git, GitOutput, PASSWORD, StatusCode, TestServer};
use rbit_services::{CreateRepository, Services, SignUp, ssh_keys::AddSshKey};
use rbit_storage::{Database, KvStore};

const BANNER: &str = "This repository has been archived.";

#[tokio::test]
async fn archived_repositories_over_http() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "demo", "public").await;
    let sample = git.sample();
    let remote = server.git_url_as("alice", "demo", "alice", PASSWORD);
    git.push_sample(&sample, &remote);
    alice
        .post("/alice/demo/pulls", &[("base", "main"), ("head", "feature"), ("title", "Feature")])
        .await
        .expect_redirect("/alice/demo/pull/1");

    // Only administrators archive, from the danger zone.
    alice.get("/alice/demo/settings").await.expect_contains(&["Archive this repository", "action=\"/alice/demo/settings/archive\""]);
    bob.post("/alice/demo/settings/archive", &[("archived", "true")]).await.expect_status(StatusCode::NOT_FOUND);
    let forged = alice.post_with("/alice/demo/settings/archive", &[("archived", "true")], &[("Origin", "https://evil.example")]).await;
    forged.expect_status(StatusCode::FORBIDDEN);
    alice.post("/alice/demo/settings/archive", &[("archived", "true")]).await.expect_redirect("/alice/demo");
    alice.get("/alice/demo").await.expect_contains(&[BANNER]);
    alice.get("/alice/demo/settings").await.expect_contains(&["Unarchive this repository"]);

    // Pushes are refused, every ref and with the reason.
    git.commit_file(&sample.dir, "late.txt", "too late\n");
    let refused = git.fails(&sample.dir, &["push", &remote, "main"]);
    assert!(refused.contains(ARCHIVED_READ_ONLY), "git prints why:\n{refused}");
    let refused = git.fails(&sample.dir, &["push", &remote, "main:refs/heads/new"]);
    assert!(refused.contains("archived"), "new branches too:\n{refused}");

    // Merges, new issues and comments are refused too.
    let merged = alice.post("/alice/demo/pull/1/merge", &[("strategy", "merge")]).await;
    merged.expect_status(StatusCode::BAD_REQUEST).expect_contains(&["archived"]);
    let opened = alice.post("/alice/demo/issues", &[("title", "Bug"), ("body", "")]).await;
    assert!(opened.status.is_client_error(), "no new issues: {}", opened.status);
    let commented = bob.post("/alice/demo/issues/1/comments", &[("body", "Still?")]).await;
    assert!(commented.status.is_client_error(), "no comments: {}", commented.status);
    let pull = alice.get("/alice/demo/pull/1").await;
    assert!(!pull.contains("Create a merge commit"), "the merge button is hidden");

    // Reading, cloning, forking and starring keep working.
    let clone = git.clone(&server.git_url("alice", "demo"));
    assert!(clone.join("README.md").exists());
    bob.post("/alice/demo/star", &[("action", "star")]).await.expect_redirect("/alice/demo");
    bob.post("/alice/demo/fork", &[("owner", "bob"), ("name", "")]).await.expect_redirect("/bob/demo");
    let fork = server.git_url_as("bob", "demo", "bob", PASSWORD);
    git.ok(&sample.dir, &["push", "--quiet", &fork, "main"]);
    bob.get("/bob/demo").await.expect_status(StatusCode::OK);
    assert!(!bob.get("/bob/demo").await.contains(BANNER), "the fork is not archived");

    // Unarchiving restores writes.
    alice.post("/alice/demo/settings/archive", &[("archived", "false")]).await.expect_redirect("/alice/demo");
    assert!(!alice.get("/alice/demo").await.contains(BANNER));
    git.ok(&sample.dir, &["push", "--quiet", &remote, "main"]);
    alice.post("/alice/demo/pull/1/merge", &[("strategy", "merge")]).await.expect_redirect("/alice/demo/pull/1");
    alice.post("/alice/demo/issues", &[("title", "Bug"), ("body", "")]).await.expect_status(StatusCode::SEE_OTHER);
}

fn openssh_available() -> bool {
    let found = |tool: &str| Command::new(tool).arg("-V").output().is_ok() || Command::new(tool).arg("-?").output().is_ok();
    if found("ssh") && found("ssh-keygen") {
        return true;
    }
    eprintln!("skipping archived repositories over SSH: the OpenSSH client (`ssh`, `ssh-keygen`) is not installed");
    false
}

fn forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Runs `git` in `dir` off the async workers (the server runs on them), with `ssh` as the
/// transport for `ssh://` remotes.
async fn git_over_ssh(git: &Arc<Git>, ssh: &str, dir: &Path, args: &[&str]) -> GitOutput {
    let mut command = git.command(dir);
    command.args(args).env("GIT_SSH_COMMAND", ssh);
    let output = tokio::task::spawn_blocking(move || command.output().expect("run git")).await.unwrap();
    GitOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn archived_repositories_refuse_pushes_over_ssh() {
    if !openssh_available() {
        return;
    }
    // HTTP and SSH on ephemeral ports, sharing one set of services (as `rbit serve` does).
    let dir = tempfile::tempdir().unwrap();
    let http = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http_addr = http.local_addr().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = dir.path().join("data");
    config.server.public_url = format!("http://{http_addr}");
    config.ssh.enabled = true;
    config.ssh.bind = "127.0.0.1:0".parse().unwrap();
    let config = Arc::new(config);
    std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
    let app = rbit_web::router(rbit_web::AppState::new(Arc::clone(&config), services.clone()));
    let server = tokio::spawn(async move { axum::serve(http, app.into_make_service_with_connect_info::<SocketAddr>()).await });
    let ssh_server = rbit_ssh::start(Arc::clone(&config), services.clone()).await.unwrap();
    let port = ssh_server.local_addr().port();

    let signup = SignUp { username: "alice".into(), email: "alice@example.com".into(), password: "correct horse battery".into() };
    let alice = services.accounts.create_user(signup, Some(false)).await.unwrap();
    let input = CreateRepository { name: "demo".into(), description: String::new(), visibility: Visibility::Private };
    services.repositories.create(&alice, input).await.unwrap();
    let key: PathBuf = dir.path().join("alice_key");
    let keygen = Command::new("ssh-keygen").args(["-q", "-t", "ed25519", "-N", "", "-C", "alice", "-f"]).arg(&key).output().unwrap();
    assert!(keygen.status.success());
    let public = std::fs::read_to_string(key.with_extension("pub")).unwrap();
    services.ssh_keys.add_user_key(&alice, AddSshKey { title: String::new(), key: public }).await.unwrap();
    let ssh = format!(
        "ssh -F none -i '{}' -o IdentitiesOnly=yes -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile='{}' -o LogLevel=ERROR",
        forward_slashes(&key),
        forward_slashes(&dir.path().join("known_hosts"))
    );
    let remote = format!("ssh://git@127.0.0.1:{port}/alice/demo.git");

    let git = Arc::new(Git::new());
    let sample = git.sample();
    let pushed = git_over_ssh(&git, &ssh, &sample.dir, &["push", &remote, "main"]).await;
    assert!(pushed.success, "push over SSH failed:\n{}", pushed.stderr);

    let set_archived = async |archived: bool| {
        let repo = services.repositories.open(Some(&alice), "alice", "demo").await.unwrap();
        assert!(services.repo_settings.set_archived(&alice, &repo, archived).await.unwrap());
    };
    set_archived(true).await;
    git.commit_file(&sample.dir, "late.txt", "too late\n");
    let refused = git_over_ssh(&git, &ssh, &sample.dir, &["push", &remote, "main"]).await;
    assert!(!refused.success, "the push went through");
    assert!(refused.stderr.contains(ARCHIVED_READ_ONLY), "git prints why:\n{}", refused.stderr);
    let clone = git.fresh_path("ssh-clone");
    let clone_arg = clone.to_string_lossy().into_owned();
    let cloned = git_over_ssh(&git, &ssh, dir.path(), &["clone", "--quiet", &remote, &clone_arg]).await;
    assert!(cloned.success, "cloning an archived repository over SSH failed:\n{}", cloned.stderr);

    set_archived(false).await;
    let pushed = git_over_ssh(&git, &ssh, &sample.dir, &["push", &remote, "main"]).await;
    assert!(pushed.success, "push after unarchiving failed:\n{}", pushed.stderr);

    ssh_server.shutdown().await;
    server.abort();
    services.shutdown().await;
    database.close().await;
}
