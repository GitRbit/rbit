//! Branch protection over SSH, end to end: a real server on an ephemeral port, driven by
//! the real `ssh`, `ssh-keygen` and `git` command-line tools. The same scenarios over
//! smart HTTP live in `rbit-e2e/tests/e2e/protection.rs`.
//!
//! Milestone 2 workstream B (branch protection) owns this file. Skipped (with a message)
//! when OpenSSH is not installed.

use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::Arc,
};

use rbit_core::{Config, User, Visibility, membership::Permission, protection::RuleSettings};
use rbit_services::{
    CreateRepository, Services,
    audit::AuditAction,
    protection::ProtectionInput,
    ssh_keys::{AddDeployKey, AddSshKey},
};
use rbit_storage::{Database, KvStore, NewUser};

const NULL_DEVICE: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

struct Harness {
    dir: tempfile::TempDir,
    database: Database,
    services: Services,
    port: u16,
    _server: rbit_ssh::RunningSshServer,
}

impl Harness {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        config.ssh.enabled = true;
        config.ssh.bind = "127.0.0.1:0".parse().unwrap();
        let config = Arc::new(config);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let server = rbit_ssh::start(config, services.clone()).await.unwrap();
        let port = server.local_addr().port();
        Self { dir, database, services, port, _server: server }
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
        let output = Command::new("ssh-keygen").args(["-q", "-t", "ed25519", "-N", "", "-C", name, "-f"]).arg(&path).output().unwrap();
        assert!(output.status.success(), "ssh-keygen failed: {}", String::from_utf8_lossy(&output.stderr));
        let public = std::fs::read_to_string(path.with_extension("pub")).unwrap();
        (path, public)
    }

    /// A user with an SSH key; returns the user and the private key.
    async fn user_with_key(&self, name: &str) -> (User, PathBuf) {
        let user = self.user(name).await;
        let (key, public) = self.key(&format!("{name}_key"));
        self.services.ssh_keys.add_user_key(&user, AddSshKey { title: String::new(), key: public }).await.unwrap();
        (user, key)
    }

    fn url(&self) -> String {
        format!("ssh://git@127.0.0.1:{}/alice/project.git", self.port)
    }

    fn ssh_command(&self, key: &Path) -> String {
        let known_hosts = forward_slashes(&self.dir.path().join("known_hosts"));
        format!(
            "ssh -F none -i '{}' -p {} -o IdentitiesOnly=yes -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile='{known_hosts}' -o LogLevel=ERROR",
            forward_slashes(key),
            self.port,
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

    async fn ok(&self, key: &Path, dir: &Path, args: &[&str]) -> String {
        let output = self.git(key, dir, args).await;
        assert!(output.status.success(), "git {args:?} failed:\n{}", stderr(&output));
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Runs `git` expecting failure; returns its stderr.
    async fn fails(&self, key: &Path, dir: &Path, args: &[&str]) -> String {
        let output = self.git(key, dir, args).await;
        assert!(!output.status.success(), "git {args:?} unexpectedly succeeded:\n{}", stderr(&output));
        stderr(&output)
    }

    /// Clones the project with `key` into a fresh directory.
    async fn clone(&self, key: &Path, name: &str) -> PathBuf {
        let parent = self.dir.path().join(name);
        std::fs::create_dir_all(&parent).unwrap();
        self.ok(key, &parent, &["clone", "-q", &self.url(), "project"]).await;
        parent.join("project")
    }

    /// Commits a new file in `dir`.
    async fn commit(&self, key: &Path, dir: &Path, file: &str) {
        std::fs::write(dir.join(file), format!("{file}\n")).unwrap();
        self.ok(key, dir, &["add", file]).await;
        self.ok(key, dir, &["commit", "-q", "-m", &format!("Add {file}")]).await;
    }

    /// The server's id for `branch`, if it exists.
    async fn remote(&self, key: &Path, dir: &Path, branch: &str) -> Option<String> {
        let listed = self.ok(key, dir, &["ls-remote", "origin", &format!("refs/heads/{branch}")]).await;
        listed.split_whitespace().next().map(str::to_owned)
    }

    async fn head(&self, key: &Path, dir: &Path) -> String {
        self.ok(key, dir, &["rev-parse", "HEAD"]).await.trim().to_owned()
    }
}

fn forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn openssh_available() -> bool {
    let found = |tool: &str| Command::new(tool).arg("-V").output().is_ok() || Command::new(tool).arg("-?").output().is_ok();
    if found("ssh") && found("ssh-keygen") {
        return true;
    }
    eprintln!("skipping branch protection over SSH: the OpenSSH client (`ssh`, `ssh-keygen`) is not installed");
    false
}

fn rule(pattern: &str, settings: RuleSettings, users: &[&str]) -> ProtectionInput {
    ProtectionInput {
        pattern: pattern.to_owned(),
        settings,
        users: users.iter().map(|user| (*user).to_owned()).collect(),
        teams: Vec::new(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn protected_branches_over_ssh() {
    if !openssh_available() {
        return;
    }
    let harness = Harness::start().await;
    let (alice, alice_key) = harness.user_with_key("alice").await;
    let (_bob, bob_key) = harness.user_with_key("bob").await;
    let (_carol, carol_key) = harness.user_with_key("carol").await;
    let repo = CreateRepository { name: "project".to_owned(), description: String::new(), visibility: Visibility::Private };
    harness.services.repositories.create(&alice, repo).await.unwrap();
    let ctx = harness.services.repositories.open(Some(&alice), "alice", "project").await.unwrap();
    for name in ["bob", "carol"] {
        harness.services.collaborators.add(&alice, &ctx, name, Permission::Write).await.unwrap();
    }
    let (deploy_key, deploy_public) = harness.key("deploy_key");
    let deploy = AddDeployKey { title: "ci".to_owned(), key: deploy_public, allow_write: true };
    harness.services.ssh_keys.add_deploy_key(&alice, &ctx, deploy).await.unwrap();

    // main and feature exist; main is protected with the defaults.
    let work = harness.clone(&alice_key, "alice").await;
    harness.commit(&alice_key, &work, "README.md").await;
    harness.ok(&alice_key, &work, &["push", "-q", "origin", "HEAD:main", "HEAD:feature"]).await;
    let protection = &harness.services.protection;
    let main_rule = protection.create(&alice, &ctx, &rule("main", RuleSettings::default(), &[])).await.unwrap();
    let main = harness.head(&alice_key, &work).await;

    // A force push to main is rejected; main is unchanged.
    harness.ok(&alice_key, &work, &["commit", "-q", "--amend", "-m", "Rewritten"]).await;
    let refused = harness.fails(&alice_key, &work, &["push", "--force", "origin", "HEAD:main"]).await;
    assert!(refused.contains("non-fast-forward"), "{refused}");
    assert_eq!(harness.remote(&alice_key, &work, "main").await, Some(main.clone()));

    // Deleting it is rejected (a deletion-only push sends no pack and keeps the channel open).
    let refused = harness.fails(&alice_key, &work, &["push", "origin", ":main"]).await;
    assert!(refused.contains("protected branch: deleting it is not allowed"), "{refused}");
    assert!(refused.contains("Rbit: refs/heads/main"), "{refused}");
    assert_eq!(harness.remote(&alice_key, &work, "main").await, Some(main.clone()));

    // An unprotected branch can be force-pushed in a separate push, and deleted.
    harness.ok(&alice_key, &work, &["push", "-q", "--force", "origin", "HEAD:feature"]).await;
    harness.ok(&alice_key, &work, &["push", "-q", "origin", ":feature"]).await;
    assert_eq!(harness.remote(&alice_key, &work, "feature").await, None);

    // refs/pull/ belongs to Rbit, whoever pushes.
    let refused = harness.fails(&alice_key, &work, &["push", "origin", "HEAD:refs/pull/1/head"]).await;
    assert!(refused.contains("refs/pull/ is managed by Rbit"), "{refused}");

    // Restricted pushers: carol may push, bob and the deploy key may not.
    let restricted = RuleSettings { restrict_pushers: true, ..RuleSettings::default() };
    protection.update(&alice, &ctx, main_rule.id, &rule("main", restricted, &["carol"])).await.unwrap();
    for (key, name) in [(&bob_key, "bob"), (&deploy_key, "deploy")] {
        let checkout = harness.clone(key, name).await;
        harness.commit(key, &checkout, &format!("{name}.txt")).await;
        let refused = harness.fails(key, &checkout, &["push", "origin", "main"]).await;
        assert!(refused.contains("protected branch: you are not allowed to push to it"), "{name}: {refused}");
    }
    let carols = harness.clone(&carol_key, "carol").await;
    harness.commit(&carol_key, &carols, "carol.txt").await;
    harness.ok(&carol_key, &carols, &["push", "-q", "origin", "main"]).await;
    let carol_head = harness.head(&carol_key, &carols).await;
    assert_eq!(harness.remote(&carol_key, &carols, "main").await, Some(carol_head.clone()));

    // A direct push to a branch that requires pull requests is rejected (creating it is fine).
    let reviewed = RuleSettings { require_pull_request: true, required_approvals: 1, ..RuleSettings::default() };
    protection.create(&alice, &ctx, &rule("release/*", reviewed, &[])).await.unwrap();
    harness.ok(&carol_key, &carols, &["push", "-q", "origin", "main:release/1.0"]).await;
    harness.commit(&carol_key, &carols, "fix.txt").await;
    let refused = harness.fails(&carol_key, &carols, &["push", "origin", "HEAD:release/1.0"]).await;
    assert!(refused.contains("protected branch: changes must go through a pull request"), "{refused}");

    // All or nothing: the allowed creation of topic is declined along with release/1.0.
    let refused = harness.fails(&carol_key, &carols, &["push", "origin", "HEAD:topic", "HEAD:release/1.0"]).await;
    assert!(refused.contains("push declined: another ref in this push was refused"), "{refused}");
    assert_eq!(harness.remote(&carol_key, &carols, "topic").await, None);

    // With bypass the administrator can force-push main, and the audit log shows it.
    let bypassing = RuleSettings { allow_admin_bypass: true, ..restricted };
    protection.update(&alice, &ctx, main_rule.id, &rule("main", bypassing, &["carol"])).await.unwrap();
    harness.ok(&alice_key, &work, &["fetch", "-q", "origin"]).await;
    harness.ok(&alice_key, &work, &["reset", "-q", "--hard", "origin/main"]).await;
    harness.ok(&alice_key, &work, &["commit", "-q", "--amend", "-m", "Rewritten by the admin"]).await;
    harness.ok(&alice_key, &work, &["push", "-q", "--force", "origin", "HEAD:main"]).await;
    let forced = harness.head(&alice_key, &work).await;
    assert_eq!(harness.remote(&alice_key, &work, "main").await, Some(forced));
    let audited = harness.services.audit.recent_for(&alice, 50).await.unwrap();
    let bypass = audited.iter().find(|event| event.action == AuditAction::BranchProtectionBypassed).expect("the bypass is audited");
    assert!(bypass.detail.starts_with("refs/heads/main"), "{}", bypass.detail);
    assert_eq!(bypass.target_name.as_deref(), Some("alice/project"));
    assert_ne!(Some(carol_head), harness.remote(&alice_key, &work, "main").await);
}
