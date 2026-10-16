//! Git over SSH end to end: a real server on an ephemeral port with a temporary data
//! directory, driven by the real `ssh`, `ssh-keygen` and `git` command-line tools.
//!
//! Skipped (with a message) when OpenSSH is not installed.

use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::Arc,
};

use rustit_core::{Config, User, Visibility};
use rustit_services::{
    CreateRepository, DomainEvent, Services,
    ssh_keys::{AddDeployKey, AddSshKey},
};
use rustit_storage::{Database, KvStore, NewUser};

const NULL_DEVICE: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

struct Harness {
    dir: tempfile::TempDir,
    database: Database,
    services: Services,
    port: u16,
    server: Option<rustit_ssh::RunningSshServer>,
}

impl Harness {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rustit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rustit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        config.ssh.enabled = true;
        config.ssh.bind = "127.0.0.1:0".parse().unwrap();
        let config = Arc::new(config);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let server = rustit_ssh::start(config, services.clone()).await.unwrap();
        let port = server.local_addr().port();
        Self { dir, database, services, port, server: Some(server) }
    }

    async fn user(&self, name: &str) -> User {
        let new = NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        };
        self.database.accounts().create_user(new).await.unwrap()
    }

    /// Generates a key pair with `ssh-keygen`; returns the private key path and public key.
    fn key(&self, name: &str) -> (PathBuf, String) {
        let path = self.dir.path().join(name);
        run(Command::new("ssh-keygen").args(["-q", "-t", "ed25519", "-N", "", "-C", name, "-f"]).arg(&path), true);
        let public = std::fs::read_to_string(path.with_extension("pub")).unwrap();
        (path, public)
    }

    fn url(&self, path: &str) -> String {
        format!("ssh://git@127.0.0.1:{}/{path}", self.port)
    }

    /// A throwaway `known_hosts`, so the test never touches the user's. (A null device
    /// would do on most systems, but MSYS's `ssh` on Windows creates a file called `NUL`.)
    fn known_hosts(&self) -> String {
        forward_slashes(&self.dir.path().join("known_hosts"))
    }

    /// `GIT_SSH_COMMAND` for `key`. Forward slashes: git hands this string to a shell.
    /// `-F none` ignores the user's `~/.ssh/config`.
    fn ssh_command(&self, key: &Path) -> String {
        format!(
            "ssh -F none -i '{}' -p {} -o IdentitiesOnly=yes -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile='{}' -o LogLevel=ERROR",
            forward_slashes(key),
            self.port,
            self.known_hosts()
        )
    }

    /// Runs `git` with `key`; returns the output whether or not it succeeded.
    async fn git(&self, key: &Path, dir: &Path, args: &[&str]) -> Output {
        let mut command = Command::new("git");
        command
            .args(args)
            .current_dir(dir)
            .env("GIT_SSH_COMMAND", self.ssh_command(key))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", NULL_DEVICE)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com");
        tokio::task::spawn_blocking(move || command.output().unwrap()).await.unwrap()
    }

    /// Runs `ssh git@127.0.0.1 [command]` with `key`.
    async fn ssh(&self, key: &Path, remote_command: Option<&str>) -> Output {
        let mut command = Command::new("ssh");
        command.current_dir(self.dir.path()).args(["-F", "none", "-i"]).arg(key).args([
            "-p",
            &self.port.to_string(),
            "-T",
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            &format!("UserKnownHostsFile={}", self.known_hosts()),
            "-o",
            "LogLevel=ERROR",
            "git@127.0.0.1",
        ]);
        if let Some(remote_command) = remote_command {
            command.arg(remote_command);
        }
        tokio::task::spawn_blocking(move || command.output().unwrap()).await.unwrap()
    }

    fn workdir(&self, name: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}

fn forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn run(command: &mut Command, must_succeed: bool) -> Output {
    let output = command.output().unwrap();
    if must_succeed {
        assert!(output.status.success(), "{command:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    output
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[track_caller]
fn assert_ok(output: &Output) {
    assert!(output.status.success(), "expected success; stderr:\n{}", stderr(output));
}

#[track_caller]
fn assert_refused(output: &Output, message: &str) {
    assert!(!output.status.success(), "expected failure; stderr:\n{}", stderr(output));
    assert!(stderr(output).contains(message), "expected {message:?} in stderr:\n{}", stderr(output));
}

fn openssh_available() -> bool {
    let found = |tool: &str| Command::new(tool).arg("-V").output().is_ok() || Command::new(tool).arg("-?").output().is_ok();
    if found("ssh") && found("ssh-keygen") {
        return true;
    }
    eprintln!("skipping Git-over-SSH test: the OpenSSH client (`ssh`, `ssh-keygen`) is not installed");
    false
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn clone_push_and_access_control_over_ssh() {
    if !openssh_available() {
        return;
    }
    let harness = Harness::start().await;
    let alice = harness.user("alice").await;
    let bob = harness.user("bob").await;
    let repo = CreateRepository { name: "project".to_owned(), description: String::new(), visibility: Visibility::Private };
    harness.services.repositories.create(&alice, repo).await.unwrap();

    let (alice_key, alice_public) = harness.key("alice_key");
    let (bob_key, bob_public) = harness.key("bob_key");
    let (deploy_key, deploy_public) = harness.key("deploy_key");
    let (stranger_key, _) = harness.key("stranger_key");
    harness.services.ssh_keys.add_user_key(&alice, AddSshKey { title: String::new(), key: alice_public }).await.unwrap();
    harness.services.ssh_keys.add_user_key(&bob, AddSshKey { title: String::new(), key: bob_public }).await.unwrap();

    // `ssh git@host` greets the user and offers no shell.
    let greeting = harness.ssh(&alice_key, None).await;
    assert_refused(&greeting, "Hi alice! You've successfully authenticated");
    assert_refused(&harness.ssh(&alice_key, Some("ls -la")).await, "only Git commands are available");
    assert_refused(&harness.ssh(&alice_key, Some("git-upload-pack '../../etc'")).await, "owner/repository");

    // Clone the empty repository, commit, push.
    let work = harness.workdir("work");
    assert_ok(&harness.git(&alice_key, &work, &["clone", &harness.url("alice/project.git"), "project"]).await);
    let checkout = work.join("project");
    std::fs::write(checkout.join("README.md"), "# project\n").unwrap();
    assert_ok(&harness.git(&alice_key, &checkout, &["add", "README.md"]).await);
    assert_ok(&harness.git(&alice_key, &checkout, &["commit", "-q", "-m", "Initial commit"]).await);

    let mut events = harness.services.events.subscribe();
    assert_ok(&harness.git(&alice_key, &checkout, &["push", "-q", "origin", "HEAD:refs/heads/main"]).await);
    let ctx = harness.services.repositories.open(Some(&alice), "alice", "project").await.unwrap();
    let event = tokio::time::timeout(std::time::Duration::from_secs(10), events.recv()).await.unwrap().unwrap();
    assert_eq!(event, DomainEvent::RepositoryPushed { repo: ctx.repo.repo.id });
    assert!(ctx.repo.repo.pushed_at.is_some(), "the push is recorded");

    // Clone it back, spelling the path without the leading slash's URL form and without `.git`.
    let copy = harness.workdir("copy");
    assert_ok(&harness.git(&alice_key, &copy, &["clone", "-q", &harness.url("alice/project"), "again"]).await);
    assert_eq!(std::fs::read_to_string(copy.join("again").join("README.md")).unwrap(), "# project\n");

    // Others cannot see the private repository, and unknown keys cannot sign in.
    let other = harness.workdir("other");
    assert_refused(&harness.git(&bob_key, &other, &["clone", &harness.url("alice/project.git")]).await, "not found");
    assert_refused(&harness.git(&stranger_key, &other, &["clone", &harness.url("alice/project.git")]).await, "Permission denied");

    // A read-only deploy key can clone but not push.
    let ctx = harness.services.repositories.open(Some(&alice), "alice", "project").await.unwrap();
    let input = AddDeployKey { title: "ci".to_owned(), key: deploy_public, allow_write: false };
    harness.services.ssh_keys.add_deploy_key(&alice, &ctx, input).await.unwrap();
    let ci = harness.workdir("ci");
    assert_ok(&harness.git(&deploy_key, &ci, &["clone", "-q", &harness.url("alice/project.git"), "project"]).await);
    let ci_checkout = ci.join("project");
    std::fs::write(ci_checkout.join("ci.txt"), "built\n").unwrap();
    assert_ok(&harness.git(&deploy_key, &ci_checkout, &["add", "ci.txt"]).await);
    assert_ok(&harness.git(&deploy_key, &ci_checkout, &["commit", "-q", "-m", "CI"]).await);
    assert_refused(&harness.git(&deploy_key, &ci_checkout, &["push", "-q", "origin", "HEAD:refs/heads/main"]).await, "read-only");
    let bob_repo = CreateRepository { name: "tools".to_owned(), description: String::new(), visibility: Visibility::Public };
    harness.services.repositories.create(&bob, bob_repo).await.unwrap();
    assert_refused(&harness.git(&deploy_key, &ci, &["clone", &harness.url("bob/tools.git")]).await, "not found");

    // Suspended users are refused.
    harness.database.admin().set_suspended(alice.id(), true).await.unwrap();
    assert_refused(&harness.git(&alice_key, &other, &["clone", &harness.url("alice/project.git")]).await, "Permission denied");

    let mut harness = harness;
    harness.server.take().unwrap().shutdown().await;
}
