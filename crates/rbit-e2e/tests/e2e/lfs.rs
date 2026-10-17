//! Git LFS with the real `git lfs` client: track, push, clone fresh, locks, a fork, and
//! the blob page over HTTP; and the same over SSH, where the client asks
//! `git-lfs-authenticate` for the HTTP endpoint. Skipped (with a message) when Git LFS
//! (or, for SSH, OpenSSH) is not installed.
//!
//! Milestone 3 workstream L (Git LFS) owns this file.

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use rbit_core::{Config, Visibility};
use rbit_e2e::{Git, GitOutput, StatusCode, TestServer};
use rbit_services::{CreateRepository, Services, SignUp, ssh_keys::AddSshKey};
use rbit_storage::{Database, KvStore};

fn git_lfs_available() -> bool {
    let found = Command::new("git").args(["lfs", "version"]).output().is_ok_and(|output| output.status.success());
    if !found {
        eprintln!("skipping Git LFS test: `git lfs` is not installed");
    }
    found
}

/// 300 KiB that do not compress to nothing and are not valid UTF-8.
fn binary_content(seed: u8) -> Vec<u8> {
    (0..300 * 1024u32).map(|i| i.wrapping_mul(2_654_435_761).to_le_bytes()[2] ^ seed).collect()
}

/// A fresh working copy with `*.bin` tracked by Git LFS and one tracked file committed.
fn lfs_working_copy(git: &Git, content: &[u8]) -> std::path::PathBuf {
    let dir = git.fresh_path("lfs");
    std::fs::create_dir_all(&dir).unwrap();
    git.ok(&dir, &["init", "--quiet", "--initial-branch=main"]);
    git.ok(&dir, &["lfs", "track", "*.bin"]);
    std::fs::write(dir.join("design.bin"), content).unwrap();
    std::fs::write(dir.join("README.md"), "# Assets\n").unwrap();
    git.ok(&dir, &["add", "--all"]);
    git.ok(&dir, &["commit", "--quiet", "-m", "Add the design"]);
    dir
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

#[tokio::test]
async fn git_lfs_pushes_clones_locks_and_forks() {
    if !git_lfs_available() {
        return;
    }
    let server = TestServer::start();
    let git = Git::new();
    // Filters for the isolated home configuration (not the user's).
    let setup = git.fresh_path("setup");
    std::fs::create_dir_all(&setup).unwrap();
    git.ok(&setup, &["lfs", "install", "--skip-repo"]);
    let _root = server.sign_up("root").await;
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "assets", "public").await;
    let alices_token = alice.create_token("lfs", "write").await;
    let bobs_token = bob.create_token("lfs", "write").await;
    let remote = server.git_url_as("alice", "assets", "alice", &alices_token);

    let content = binary_content(7);
    let work = lfs_working_copy(&git, &content);
    let pushed = git.run(&work, &["push", &remote, "main"]);
    assert!(pushed.success, "push failed:\n{}", pushed.stderr);
    assert!(git.ok(&work, &["lfs", "ls-files"]).contains("design.bin"));

    // What Git stores is the pointer; the server has the object.
    let pointer = git.ok(&work, &["cat-file", "-p", "HEAD:design.bin"]);
    assert!(pointer.starts_with("version https://git-lfs.github.com/spec/v1"), "{pointer}");
    let objects = server.data_dir().join("lfs");
    assert!(objects.is_dir(), "objects are stored under <data_dir>/lfs");

    // A fresh clone, anonymously (the repository is public), gets the content.
    let clone = git.clone(&server.git_url("alice", "assets"));
    assert_eq!(read(&clone.join("design.bin")), content, "the clone has the object, not the pointer");

    // The blob page shows the object; raw serves it.
    server.client().get("/alice/assets/blob/main/design.bin").await.expect_status(StatusCode::OK).expect_contains(&[
        "Stored with Git LFS",
        "300.0 KB",
        "Binary file not shown",
    ]);
    let raw = reqwest::get(server.url("/alice/assets/raw/main/design.bin")).await.unwrap();
    assert_eq!(raw.status(), StatusCode::OK);
    assert_eq!(raw.bytes().await.unwrap().as_ref(), content.as_slice());

    // Locks: alice locks the file, bob sees it and cannot unlock it.
    let alice_checkout = git.clone(&remote);
    git.ok(&alice_checkout, &["lfs", "lock", "design.bin"]);
    let locks = git.ok(&alice_checkout, &["lfs", "locks"]);
    assert!(locks.contains("design.bin") && locks.contains("alice"), "{locks}");
    let collaborators = "/alice/assets/settings/collaborators";
    alice.post(collaborators, &[("username", "bob"), ("permission", "write")]).await.expect_redirect(collaborators);
    let bob_checkout = git.clone(&server.git_url_as("alice", "assets", "bob", &bobs_token));
    git.fails(&bob_checkout, &["lfs", "unlock", "design.bin"]);
    assert!(git.ok(&bob_checkout, &["lfs", "locks"]).contains("design.bin"), "the lock stays");
    git.ok(&alice_checkout, &["lfs", "unlock", "design.bin"]);
    assert!(!git.ok(&alice_checkout, &["lfs", "locks"]).contains("design.bin"));

    // A fork: cloning it brings the objects along, and pushing to it adds new ones.
    bob.post("/alice/assets/fork", &[("owner", "bob"), ("name", "")]).await.expect_redirect("/bob/assets");
    let fork_remote = server.git_url_as("bob", "assets", "bob", &bobs_token);
    let fork_clone = git.clone(&fork_remote);
    assert_eq!(read(&fork_clone.join("design.bin")), content);
    let more = binary_content(9);
    std::fs::write(fork_clone.join("more.bin"), &more).unwrap();
    git.ok(&fork_clone, &["add", "more.bin"]);
    git.ok(&fork_clone, &["commit", "--quiet", "-m", "More"]);
    let pushed = git.run(&fork_clone, &["push", "origin", "main"]);
    assert!(pushed.success, "push to the fork failed:\n{}", pushed.stderr);
    let again = git.clone(&server.git_url("bob", "assets"));
    assert_eq!(read(&again.join("more.bin")), more);
}

fn openssh_available() -> bool {
    let found = |tool: &str| Command::new(tool).arg("-V").output().is_ok() || Command::new(tool).arg("-?").output().is_ok();
    if found("ssh") && found("ssh-keygen") {
        return true;
    }
    eprintln!("skipping Git LFS over SSH: the OpenSSH client (`ssh`, `ssh-keygen`) is not installed");
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
async fn git_lfs_over_ssh_uses_git_lfs_authenticate() {
    if !git_lfs_available() || !openssh_available() {
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
    let alice = services.accounts.create_user(signup, Some(true)).await.unwrap();
    let input = CreateRepository { name: "assets".into(), description: String::new(), visibility: Visibility::Private };
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
    let remote = format!("ssh://git@127.0.0.1:{port}/alice/assets.git");

    let git = Arc::new(Git::new());
    let setup = git.fresh_path("setup");
    std::fs::create_dir_all(&setup).unwrap();
    git.ok(&setup, &["lfs", "install", "--skip-repo"]);
    let content = binary_content(3);
    let work = lfs_working_copy(&git, &content);
    let pushed = git_over_ssh(&git, &ssh, &work, &["push", &remote, "main"]).await;
    assert!(pushed.success, "push over SSH failed:\n{}", pushed.stderr);

    let clone = git.fresh_path("ssh-clone");
    let clone_arg = clone.to_string_lossy().into_owned();
    let cloned = git_over_ssh(&git, &ssh, dir.path(), &["clone", "--quiet", &remote, &clone_arg]).await;
    assert!(cloned.success, "clone over SSH failed:\n{}", cloned.stderr);
    assert_eq!(read(&clone.join("design.bin")), content, "the object came through the HTTP endpoint");

    ssh_server.shutdown().await;
    server.abort();
    services.shutdown().await;
    database.close().await;
}
