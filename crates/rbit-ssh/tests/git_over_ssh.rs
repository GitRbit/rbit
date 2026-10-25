//! Git over SSH end to end: a real server on an ephemeral port with a temporary data
//! directory, driven by the real `ssh`, `ssh-keygen` and `git` command-line tools.
//!
//! Skipped (with a message) when OpenSSH is not installed.

use std::{
    future::Future,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::Arc,
};

use rbit_core::{Config, User, Visibility};
use rbit_services::{
    CreateRepository, DomainEvent, Services,
    ssh_keys::{AddDeployKey, AddSshKey},
};
use rbit_storage::{Database, KvStore, NewUser};
use russh::{
    ChannelMsg, client,
    keys::{PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate, ssh_key::private::Ed25519Keypair},
};

const NULL_DEVICE: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

struct Harness {
    dir: tempfile::TempDir,
    database: Database,
    services: Services,
    port: u16,
    server: Option<rbit_ssh::RunningSshServer>,
}

#[tokio::test]
async fn version_privacy_hides_the_pre_authentication_ssh_version() {
    use tokio::io::AsyncBufReadExt;
    let harness = Harness::start_with(|config| {
        config.updates.report_version = false;
        config.updates.check_for_updates = false;
    })
    .await;
    let connection = tokio::net::TcpStream::connect(("127.0.0.1", harness.port)).await.unwrap();
    let mut reader = tokio::io::BufReader::new(connection);
    let mut banner = String::new();
    tokio::time::timeout(std::time::Duration::from_secs(5), reader.read_line(&mut banner)).await.unwrap().unwrap();
    assert_eq!(banner.trim(), "SSH-2.0-rbit");
    drop(reader);
    let mut harness = harness;
    harness.server.take().unwrap().shutdown().await;
}

impl Harness {
    async fn start() -> Self {
        Self::start_with(|_| {}).await
    }

    fn start_with(configure: impl FnOnce(&mut Config)) -> impl Future<Output = Self> {
        // Keep setup off callers' future stacks as the combined service configuration grows.
        Box::pin(async move {
            let dir = tempfile::tempdir().unwrap();
            let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
            let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
            let mut config = Config::default();
            config.storage.data_dir = dir.path().join("data");
            config.ssh.enabled = true;
            config.ssh.bind = "127.0.0.1:0".parse().unwrap();
            configure(&mut config);
            let config = Arc::new(config);
            std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
            let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
            let server = rbit_ssh::start(config, services.clone()).await.unwrap();
            let port = server.local_addr().port();
            Self { dir, database, services, port, server: Some(server) }
        })
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

struct PersistentClient;

#[allow(unknown_lints, clippy::unused_async_trait_impl)]
impl client::Handler for PersistentClient {
    type Error = russh::Error;

    async fn check_server_key(&mut self, _: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

async fn persistent_client(harness: &Harness, key: PrivateKey) -> client::Handle<PersistentClient> {
    let config = client::Config { inactivity_timeout: Some(std::time::Duration::from_secs(30)), ..Default::default() };
    let mut client = client::connect(Arc::new(config), ("127.0.0.1", harness.port), PersistentClient).await.unwrap();
    assert!(client.authenticate_publickey("git", PrivateKeyWithHashAlg::new(Arc::new(key), None)).await.unwrap().success());
    client
}

async fn persistent_exec(client: &client::Handle<PersistentClient>, command: &str) -> (u32, String) {
    let work = async {
        let mut channel = client.channel_open_session().await.unwrap();
        channel.exec(true, command).await.unwrap();
        let mut status = None;
        let mut output = Vec::new();
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => output.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                _ => {}
            }
        }
        (status.expect("command exited"), String::from_utf8(output).unwrap())
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), work).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn revoked_keys_cannot_run_more_commands_on_an_authenticated_connection() {
    let mut harness = Harness::start_with(|config| config.lfs.enabled = true).await;
    let owner = harness.user("alice").await;
    harness
        .services
        .repositories
        .create(&owner, CreateRepository { name: "project".to_owned(), description: String::new(), visibility: Visibility::Private })
        .await
        .unwrap();
    let repo = harness.services.repositories.open(Some(&owner), "alice", "project").await.unwrap();
    let command = "git-lfs-authenticate alice/project download";
    for deploy in [false, true] {
        let key = PrivateKey::from(Ed25519Keypair::from_seed(&[if deploy { 42 } else { 43 }; 32]));
        let public = key.public_key().to_openssh().unwrap();
        let (user_key, deploy_key) = if deploy {
            let input = AddDeployKey { title: "test".to_owned(), key: public, allow_write: false };
            (None, Some(harness.services.ssh_keys.add_deploy_key(&owner, &repo, input).await.unwrap().id))
        } else {
            let input = AddSshKey { title: "test".to_owned(), key: public };
            (Some(harness.services.ssh_keys.add_user_key(&owner, input).await.unwrap().id), None)
        };
        let client = persistent_client(&harness, key).await;
        assert_eq!(persistent_exec(&client, command).await.0, 0);
        if let Some(key) = user_key {
            harness.services.ssh_keys.delete_user_key(&owner, key).await.unwrap();
        }
        if let Some(key) = deploy_key {
            harness.services.ssh_keys.delete_deploy_key(&owner, &repo, key).await.unwrap();
        }
        let (status, message) = persistent_exec(&client, command).await;
        assert_ne!(status, 0);
        assert!(message.contains("no longer authorized"), "{message}");
        client.disconnect(russh::Disconnect::ByApplication, "done", "").await.unwrap();
    }
    harness.server.take().unwrap().shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn admin_demotion_applies_to_an_authenticated_ssh_connection() {
    let mut harness = Harness::start_with(|config| config.lfs.enabled = true).await;
    let owner = harness.user("alice").await;
    let former_admin = harness.user("bob").await;
    harness.database.admin().set_admin(owner.id(), true).await.unwrap();
    harness.database.admin().set_admin(former_admin.id(), true).await.unwrap();
    harness
        .services
        .repositories
        .create(&owner, CreateRepository { name: "private".to_owned(), description: String::new(), visibility: Visibility::Private })
        .await
        .unwrap();
    let key = PrivateKey::from(Ed25519Keypair::from_seed(&[44; 32]));
    harness
        .services
        .ssh_keys
        .add_user_key(&former_admin, AddSshKey { title: "admin".to_owned(), key: key.public_key().to_openssh().unwrap() })
        .await
        .unwrap();
    let client = persistent_client(&harness, key).await;
    let command = "git-lfs-authenticate alice/private download";
    assert_eq!(persistent_exec(&client, command).await.0, 0);
    harness.database.admin().set_admin(former_admin.id(), false).await.unwrap();
    let (status, message) = persistent_exec(&client, command).await;
    assert_ne!(status, 0);
    assert!(message.contains("not found"), "{message}");
    client.disconnect(russh::Disconnect::ByApplication, "done", "").await.unwrap();
    harness.server.take().unwrap().shutdown().await;
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn case_aliases_cannot_delete_protected_or_managed_refs_over_ssh() {
    let mut harness = Harness::start().await;
    let owner = harness.user("alice").await;
    harness
        .services
        .repositories
        .create(&owner, CreateRepository { name: "project".to_owned(), description: String::new(), visibility: Visibility::Private })
        .await
        .unwrap();
    let repo = harness.services.repositories.open(Some(&owner), "alice", "project").await.unwrap();
    let key = PrivateKey::from(Ed25519Keypair::from_seed(&[45; 32]));
    harness
        .services
        .ssh_keys
        .add_user_key(&owner, AddSshKey { title: "test".to_owned(), key: key.public_key().to_openssh().unwrap() })
        .await
        .unwrap();
    let work = harness.workdir("seed");
    let unused_key = Path::new("unused");
    assert_ok(&harness.git(unused_key, &work, &["init", "-q", "-b", "main"]).await);
    std::fs::write(work.join("README"), "hello\n").unwrap();
    assert_ok(&harness.git(unused_key, &work, &["add", "README"]).await);
    assert_ok(&harness.git(unused_key, &work, &["commit", "-q", "-m", "initial"]).await);
    assert_ok(
        &harness.git(unused_key, &work, &["push", "-q", repo.disk_path.to_str().unwrap(), "HEAD:main", "HEAD:refs/pull/1/head"]).await,
    );
    let original = repo.reader.commit_id("main".to_owned()).await.unwrap();
    let rule = rbit_services::protection::ProtectionInput {
        pattern: "main".to_owned(),
        settings: rbit_core::protection::RuleSettings::default(),
        users: Vec::new(),
        teams: Vec::new(),
        contexts: Vec::new(),
    };
    harness.services.protection.create(&owner, &repo, &rule).await.unwrap();
    let client = persistent_client(&harness, key).await;

    for name in ["refs/heads/MAIN", "refs/HEADS/main", "refs/PULL/1/head"] {
        let exchange = async {
            let mut channel = client.channel_open_session().await.unwrap();
            channel.exec(true, "git-receive-pack alice/project").await.unwrap();
            let mut advertisement = Vec::new();
            while let Some(message) = channel.wait().await {
                if let ChannelMsg::Data { data } = message {
                    advertisement.extend_from_slice(&data);
                    if advertisement.ends_with(b"0000") {
                        break;
                    }
                }
            }
            assert!(advertisement.ends_with(b"0000"));
            // Send the real old id under a different spelling, which aliases the same
            // loose ref on NTFS. A normal Git client would send a create command here.
            let mut command = rbit_transport::pkt_line::encode(&format!("{original} {} {name}\0report-status\n", "0".repeat(40)));
            command.extend_from_slice(b"0000");
            channel.data(command.as_slice()).await.unwrap();
            channel.eof().await.unwrap();
            let mut report = Vec::new();
            while let Some(message) = channel.wait().await {
                if let ChannelMsg::Data { data } = message {
                    report.extend_from_slice(&data);
                }
            }
            String::from_utf8(report).unwrap()
        };
        let report = tokio::time::timeout(std::time::Duration::from_secs(10), exchange).await.unwrap();
        assert!(report.contains(&format!("ng {name} ")), "{name}: {report}");
        assert_eq!(repo.reader.commit_id("main".to_owned()).await.unwrap(), original);
        assert_eq!(repo.reader.commit_id("refs/pull/1/head".to_owned()).await.unwrap(), original);
    }
    client.disconnect(russh::Disconnect::ByApplication, "done", "").await.unwrap();
    harness.server.take().unwrap().shutdown().await;
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
    let DomainEvent::RepositoryPushed { repo, pusher, updates } = event else { panic!("expected a push event, got {event:?}") };
    assert_eq!((repo, pusher), (ctx.repo.repo.id, Some(alice.id())));
    let refs: Vec<&str> = updates.iter().map(|update| update.name.as_str()).collect();
    assert_eq!(refs, ["refs/heads/main"], "the push reports its ref update");
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn old_names_of_renamed_repositories_keep_working_over_ssh() {
    if !openssh_available() {
        return;
    }
    let harness = Harness::start().await;
    let alice = harness.user("alice").await;
    let bob = harness.user("bob").await;
    let repo = CreateRepository { name: "before".to_owned(), description: String::new(), visibility: Visibility::Private };
    harness.services.repositories.create(&alice, repo).await.unwrap();
    let (alice_key, alice_public) = harness.key("alice_key");
    let (bob_key, bob_public) = harness.key("bob_key");
    let (deploy_key, deploy_public) = harness.key("deploy_key");
    harness.services.ssh_keys.add_user_key(&alice, AddSshKey { title: String::new(), key: alice_public }).await.unwrap();
    harness.services.ssh_keys.add_user_key(&bob, AddSshKey { title: String::new(), key: bob_public }).await.unwrap();
    let ctx = harness.services.repositories.open(Some(&alice), "alice", "before").await.unwrap();
    let input = AddDeployKey { title: "ci".to_owned(), key: deploy_public, allow_write: false };
    harness.services.ssh_keys.add_deploy_key(&alice, &ctx, input).await.unwrap();

    let work = harness.workdir("work");
    assert_ok(&harness.git(&alice_key, &work, &["clone", "-q", &harness.url("alice/before.git"), "project"]).await);
    let checkout = work.join("project");
    std::fs::write(checkout.join("README.md"), "# project\n").unwrap();
    assert_ok(&harness.git(&alice_key, &checkout, &["add", "README.md"]).await);
    assert_ok(&harness.git(&alice_key, &checkout, &["commit", "-q", "-m", "Initial commit"]).await);
    harness.services.repo_settings.rename(&ctx, "after").await.unwrap();

    // The remote still says `alice/before`: pushing and cloning there reach `alice/after`.
    assert_ok(&harness.git(&alice_key, &checkout, &["push", "-q", "origin", "HEAD:refs/heads/main"]).await);
    let copy = harness.workdir("copy");
    assert_ok(&harness.git(&alice_key, &copy, &["clone", "-q", &harness.url("alice/before"), "again"]).await);
    assert_eq!(std::fs::read_to_string(copy.join("again").join("README.md")).unwrap(), "# project\n");
    assert_ok(&harness.git(&deploy_key, &copy, &["clone", "-q", &harness.url("alice/before.git"), "ci"]).await);
    // Someone who cannot read the repository learns nothing from its old name.
    assert_refused(&harness.git(&bob_key, &copy, &["clone", &harness.url("alice/before.git"), "bob"]).await, "not found");

    // A new repository at the old name takes precedence.
    let newcomer = CreateRepository { name: "before".to_owned(), description: String::new(), visibility: Visibility::Private };
    harness.services.repositories.create(&alice, newcomer).await.unwrap();
    let fresh = harness.workdir("fresh");
    assert_ok(&harness.git(&alice_key, &fresh, &["clone", "-q", &harness.url("alice/before.git"), "new"]).await);
    assert!(!fresh.join("new").join("README.md").exists(), "the new, empty repository is served");
    // The deploy key belongs to the renamed repository, not the newcomer.
    assert_refused(&harness.git(&deploy_key, &fresh, &["clone", &harness.url("alice/before.git"), "ci"]).await, "not found");

    let mut harness = harness;
    harness.server.take().unwrap().shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn old_account_names_keep_working_over_ssh_until_reclaimed() {
    if !openssh_available() {
        return;
    }
    let harness = Harness::start().await;
    let alice = harness.user("alice").await;
    let bob = harness.user("bob").await;
    harness
        .services
        .repositories
        .create(&alice, CreateRepository { name: "project".into(), description: String::new(), visibility: Visibility::Private })
        .await
        .unwrap();
    let (key, public) = harness.key("alice_key");
    harness.services.ssh_keys.add_user_key(&alice, AddSshKey { title: String::new(), key: public }).await.unwrap();
    let work = harness.workdir("account-work");
    assert_ok(&harness.git(&key, &work, &["clone", "-q", &harness.url("alice/project.git"), "project"]).await);
    let checkout = work.join("project");
    std::fs::write(checkout.join("README.md"), "same account\n").unwrap();
    assert_ok(&harness.git(&key, &checkout, &["add", "README.md"]).await);
    assert_ok(&harness.git(&key, &checkout, &["commit", "-q", "-m", "Initial commit"]).await);
    harness.database.accounts().rename(alice.id(), &"renamed".parse().unwrap()).await.unwrap();
    // The public key and old remote still reach the original account identity.
    assert_ok(&harness.git(&key, &checkout, &["push", "-q", "origin", "HEAD:refs/heads/main"]).await);
    assert_ok(&harness.git(&key, &work, &["clone", "-q", &harness.url("alice/project.git"), "copy"]).await);
    assert_eq!(std::fs::read_to_string(work.join("copy/README.md")).unwrap(), "same account\n");
    harness.database.accounts().rename(bob.id(), &"alice".parse().unwrap()).await.unwrap();
    assert_refused(&harness.git(&key, &work, &["clone", &harness.url("alice/project.git"), "reclaimed"]).await, "not found");
    assert_ok(&harness.git(&key, &work, &["clone", "-q", &harness.url("renamed/project.git"), "current"]).await);
    let mut harness = harness;
    harness.server.take().unwrap().shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pushes_to_mirrors_are_refused_over_ssh() {
    if !openssh_available() {
        return;
    }
    let harness = Harness::start().await;
    let alice = harness.user("alice").await;
    let repo = CreateRepository { name: "copy".to_owned(), description: String::new(), visibility: Visibility::Public };
    let created = harness.services.repositories.create(&alice, repo).await.unwrap();
    let (alice_key, alice_public) = harness.key("alice_key");
    harness.services.ssh_keys.add_user_key(&alice, AddSshKey { title: String::new(), key: alice_public }).await.unwrap();

    let work = harness.workdir("work");
    assert_ok(&harness.git(&alice_key, &work, &["clone", "-q", &harness.url("alice/copy.git"), "copy"]).await);
    let checkout = work.join("copy");
    std::fs::write(checkout.join("README.md"), "# copy\n").unwrap();
    assert_ok(&harness.git(&alice_key, &checkout, &["add", "README.md"]).await);
    assert_ok(&harness.git(&alice_key, &checkout, &["commit", "-q", "-m", "Initial commit"]).await);

    // Recorded directly: no fetch runs, since the background scheduler is not started.
    harness.database.mirrors().create(created.repo.id, "https://example.com/upstream.git", 60).await.unwrap();
    let refused = harness.git(&alice_key, &checkout, &["push", "-q", "origin", "HEAD:refs/heads/main"]).await;
    assert_refused(&refused, "This repository is a mirror; push to its upstream instead");
    // Fetching from a mirror is fine.
    assert_ok(&harness.git(&alice_key, &checkout, &["fetch", "-q", "origin"]).await);

    // Once it stops mirroring it is an ordinary repository again.
    harness.database.mirrors().delete(created.repo.id).await.unwrap();
    assert_ok(&harness.git(&alice_key, &checkout, &["push", "-q", "origin", "HEAD:refs/heads/main"]).await);

    let mut harness = harness;
    harness.server.take().unwrap().shutdown().await;
}

async fn commit(harness: &Harness, key: &Path, checkout: &Path, message: &str) {
    std::fs::write(checkout.join("file.txt"), message).unwrap();
    assert_ok(&harness.git(key, checkout, &["add", "file.txt"]).await);
    assert_ok(&harness.git(key, checkout, &["commit", "-q", "-m", message]).await);
}

async fn remote_refs(harness: &Harness, key: &Path, checkout: &Path) -> String {
    let output = harness.git(key, checkout, &["ls-remote", "origin"]).await;
    assert_ok(&output);
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Pushes run in two steps (the advertisement, then a stateless receive-pack after the
/// push gate); every kind of update a client sends must work exactly as before.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_kind_of_push_works_through_the_two_step_exchange() {
    if !openssh_available() {
        return;
    }
    let harness = Harness::start().await;
    let alice = harness.user("alice").await;
    let repo = CreateRepository { name: "project".to_owned(), description: String::new(), visibility: Visibility::Public };
    harness.services.repositories.create(&alice, repo).await.unwrap();
    let (key, alice_public) = harness.key("alice_key");
    harness.services.ssh_keys.add_user_key(&alice, AddSshKey { title: String::new(), key: alice_public }).await.unwrap();

    let work = harness.workdir("work");
    assert_ok(&harness.git(&key, &work, &["clone", "-q", &harness.url("alice/project.git"), "project"]).await);
    let checkout = work.join("project");

    commit(&harness, &key, &checkout, "one").await;
    assert_ok(&harness.git(&key, &checkout, &["push", "-q", "origin", "HEAD:refs/heads/main"]).await);
    // Nothing to push: the client sends an empty command list.
    let again = harness.git(&key, &checkout, &["push", "origin", "HEAD:refs/heads/main"]).await;
    assert_ok(&again);
    assert!(stderr(&again).contains("Everything up-to-date"), "{}", stderr(&again));

    // Several refs at once, atomically; then a deletion, after which no pack follows.
    commit(&harness, &key, &checkout, "two").await;
    let both = ["push", "-q", "--atomic", "origin", "HEAD:refs/heads/main", "HEAD:refs/heads/feature"];
    assert_ok(&harness.git(&key, &checkout, &both).await);
    assert!(remote_refs(&harness, &key, &checkout).await.contains("refs/heads/feature"));
    assert_ok(&harness.git(&key, &checkout, &["push", "-q", "origin", ":refs/heads/feature"]).await);
    assert!(!remote_refs(&harness, &key, &checkout).await.contains("refs/heads/feature"), "the branch is deleted");

    // Rewritten history needs --force (nothing protects the branch yet).
    assert_ok(&harness.git(&key, &checkout, &["commit", "-q", "--amend", "-m", "two, rewritten"]).await);
    assert_refused(&harness.git(&key, &checkout, &["push", "origin", "HEAD:refs/heads/main"]).await, "non-fast-forward");
    assert_ok(&harness.git(&key, &checkout, &["push", "-q", "--force", "origin", "HEAD:refs/heads/main"]).await);
    let head = harness.git(&key, &checkout, &["rev-parse", "HEAD"]).await;
    let head = String::from_utf8_lossy(&head.stdout).trim().to_owned();
    assert!(remote_refs(&harness, &key, &checkout).await.contains(&head), "main is the rewritten commit");

    let mut harness = harness;
    harness.server.take().unwrap().shutdown().await;
}

async fn lfs_authenticate(harness: &Harness, key: &Path, operation: &str) -> Output {
    harness.ssh(key, Some(&format!("git-lfs-authenticate 'alice/assets.git' {operation}"))).await
}

/// Milestone 3: Git LFS (L). `git-lfs-authenticate` hands out the HTTP endpoint and a
/// signed header, with the access a fetch or push over SSH would have.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn git_lfs_authenticate_returns_a_signed_header() {
    if !openssh_available() {
        return;
    }
    let harness = Harness::start().await;
    let alice = harness.user("alice").await;
    let bob = harness.user("bob").await;
    let repo = CreateRepository { name: "assets".to_owned(), description: String::new(), visibility: Visibility::Private };
    harness.services.repositories.create(&alice, repo).await.unwrap();
    let (alice_key, alice_public) = harness.key("alice_key");
    let (bob_key, bob_public) = harness.key("bob_key");
    let (deploy_key, deploy_public) = harness.key("deploy_key");
    harness.services.ssh_keys.add_user_key(&alice, AddSshKey { title: String::new(), key: alice_public }).await.unwrap();
    harness.services.ssh_keys.add_user_key(&bob, AddSshKey { title: String::new(), key: bob_public }).await.unwrap();
    let ctx = harness.services.repositories.open(Some(&alice), "alice", "assets").await.unwrap();
    let input = AddDeployKey { title: "ci".to_owned(), key: deploy_public, allow_write: false };
    harness.services.ssh_keys.add_deploy_key(&alice, &ctx, input).await.unwrap();

    for (operation, write) in [("download", false), ("upload", true)] {
        let output = lfs_authenticate(&harness, &alice_key, operation).await;
        assert_ok(&output);
        let answer: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(answer["href"], "http://localhost:3000/alice/assets.git/info/lfs");
        assert_eq!(answer["expires_in"], 3600);
        let header = answer["header"]["Authorization"].as_str().unwrap();
        let grant = harness.services.lfs.verify_token(header.strip_prefix("Bearer ").unwrap()).unwrap();
        assert_eq!((grant.repo, grant.write, grant.object.clone()), (ctx.repo.repo.id, write, None));
        let granted = harness.services.lfs.open_granted(grant, "alice", "assets").await.unwrap();
        assert_eq!(granted.ctx.access.can_write(), write);
    }

    // The same refusals as Git over SSH.
    assert_refused(&lfs_authenticate(&harness, &bob_key, "download").await, "not found");
    assert_ok(&lfs_authenticate(&harness, &deploy_key, "download").await);
    assert_refused(&lfs_authenticate(&harness, &deploy_key, "upload").await, "read-only");
    assert_refused(&lfs_authenticate(&harness, &alice_key, "delete").await, "upload or download");

    let mut harness = harness;
    harness.server.take().unwrap().shutdown().await;

    let mut disabled = Harness::start_with(|config| config.lfs.enabled = false).await;
    let carol = disabled.user("carol").await;
    let (carol_key, carol_public) = disabled.key("carol_key");
    disabled.services.ssh_keys.add_user_key(&carol, AddSshKey { title: String::new(), key: carol_public }).await.unwrap();
    let refused = disabled.ssh(&carol_key, Some("git-lfs-authenticate carol/anything download")).await;
    assert_refused(&refused, "Git LFS is not enabled");
    disabled.server.take().unwrap().shutdown().await;
}

/// Runs `command` on a new channel, sends `input` and EOF; returns the exit status and output.
async fn exec_with_input(client: &client::Handle<PersistentClient>, command: &str, input: &[u8]) -> (u32, String) {
    let work = async {
        let mut channel = client.channel_open_session().await.unwrap();
        channel.exec(true, command).await.unwrap();
        channel.data(input).await.unwrap();
        channel.eof().await.unwrap();
        let mut status = None;
        let mut output = Vec::new();
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => output.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                _ => {}
            }
        }
        (status.expect("command exited"), String::from_utf8_lossy(&output).into_owned())
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), work).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn git_commands_share_the_git_process_limit() {
    let mut harness = Harness::start_with(|config| config.repository.max_git_processes = 1).await;
    let owner = harness.user("alice").await;
    harness
        .services
        .repositories
        .create(&owner, CreateRepository { name: "project".to_owned(), description: String::new(), visibility: Visibility::Private })
        .await
        .unwrap();
    let key = PrivateKey::from(Ed25519Keypair::from_seed(&[44; 32]));
    let public = key.public_key().to_openssh().unwrap();
    harness.services.ssh_keys.add_user_key(&owner, AddSshKey { title: "test".to_owned(), key: public }).await.unwrap();
    let client = persistent_client(&harness, key).await;
    let fetch = "git-upload-pack 'alice/project.git'";

    // Smart HTTP holds the only slot: SSH is refused instead of starting git.
    let held = harness.services.git_processes.try_acquire(None).unwrap();
    let (status, message) = exec_with_input(&client, fetch, b"0000").await;
    assert_ne!(status, 0);
    assert!(message.contains("busy"), "{message}");

    drop(held);
    let (status, message) = exec_with_input(&client, fetch, b"0000").await;
    assert_eq!(status, 0, "{message}");
    // The finished command gives its slot back (just after reporting its status).
    let released = async {
        while harness.services.git_processes.available() == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), released).await.expect("the slot was not released");

    client.disconnect(russh::Disconnect::ByApplication, "done", "").await.unwrap();
    harness.server.take().unwrap().shutdown().await;
}
