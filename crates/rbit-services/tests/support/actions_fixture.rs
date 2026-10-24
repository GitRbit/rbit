//! A server with Actions enabled, users, a repository with Actions on and a work tree to
//! push workflow files from, for the Actions trigger and run tests (workstream T).

#![allow(dead_code, reason = "each test module uses part of the fixture")]

use std::{path::Path, process::Command, sync::Arc};

use rbit_core::{Config, Permission, RefUpdate, Timestamp, User, Visibility, actions::Run};
use rbit_services::{ActionsService, CreateRepository, RepoContext, Services};
use rbit_storage::{Database, KvStore, NewUser};

pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .output()
        .expect("git is installed");
    assert!(output.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

pub async fn user(database: &Database, name: &str) -> User {
    database
        .accounts()
        .create_user(NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        })
        .await
        .unwrap()
}

pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub database: Database,
    pub services: Services,
    /// Owns `alice/rocket` (public, Actions on).
    pub alice: User,
    /// A writer of `alice/rocket`.
    pub carol: User,
    /// A reader of `alice/rocket`.
    pub rita: User,
    /// Nobody in particular.
    pub bob: User,
    pub work: std::path::PathBuf,
}

impl Fixture {
    pub async fn new() -> Self {
        Self::with(|_| {}).await
    }

    pub async fn with(configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        "https://git.example.com".clone_into(&mut config.server.public_url);
        config.actions.enabled = true;
        configure(&mut config);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let (alice, carol, rita, bob) =
            (user(&database, "alice").await, user(&database, "carol").await, user(&database, "rita").await, user(&database, "bob").await);
        let input = CreateRepository { name: "rocket".into(), description: String::new(), visibility: Visibility::Public };
        services.repositories.create(&alice, input).await.unwrap();
        let fixture = Self { work: dir.path().join("work"), dir, database, services, alice, carol, rita, bob };
        let repo = fixture.repo(&fixture.alice).await;
        fixture.services.collaborators.add(&fixture.alice, &repo, "carol", Permission::Write).await.unwrap();
        fixture.services.collaborators.add(&fixture.alice, &repo, "rita", Permission::Read).await.unwrap();
        fixture.database.actions_settings().set_enabled(repo.repo.repo.id, true, Timestamp::now()).await.unwrap();
        std::fs::create_dir(&fixture.work).unwrap();
        git(&fixture.work, &["init", "-q"]);
        fixture
    }

    pub fn actions(&self) -> &ActionsService {
        self.services.actions.as_ref().expect("Actions is enabled")
    }

    pub async fn repo(&self, viewer: &User) -> RepoContext {
        self.services.repositories.open(Some(viewer), "alice", "rocket").await.unwrap()
    }

    /// Writes `files` (path, contents; empty contents remove the file) and commits them.
    pub fn commit(&self, files: &[(&str, &str)], message: &str) -> String {
        for (path, contents) in files {
            let path = self.work.join(path);
            if contents.is_empty() {
                let _ = std::fs::remove_file(&path);
            } else {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, contents).unwrap();
            }
        }
        git(&self.work, &["add", "-A"]);
        git(&self.work, &["commit", "-q", "--allow-empty", "-m", message]);
        git(&self.work, &["rev-parse", "HEAD"])
    }

    /// Pushes `HEAD` to `reference` of `alice/rocket` and returns the update it made.
    pub async fn push_ref(&self, reference: &str) -> RefUpdate {
        let repo = self.repo(&self.alice).await;
        let before = repo.reader.commit_id(reference.to_owned()).await.ok();
        let path = repo.disk_path.to_str().unwrap().to_owned();
        git(&self.work, &["push", "-q", "-f", &path, &format!("HEAD:{reference}")]);
        RefUpdate { name: reference.to_owned(), before, after: Some(git(&self.work, &["rev-parse", "HEAD"])) }
    }

    /// The runs of `alice/rocket`, oldest first.
    pub async fn runs(&self) -> Vec<Run> {
        let repo = self.repo(&self.alice).await;
        let filter = rbit_storage::sqlite::actions_runs::RunFilter { limit: 500, ..Default::default() };
        let mut runs = self.database.actions_runs().list_runs(repo.repo.repo.id, &filter).await.unwrap();
        runs.reverse();
        runs
    }
}

/// A workflow on `on` with one job `build` (or the given jobs).
pub fn workflow(name: &str, on: &str) -> String {
    format!("name: {name}\non: {on}\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hi\n")
}

impl Fixture {
    /// A runner of `alice/rocket` with `labels` takes the oldest job it can, as the
    /// scheduler would; `seed` keeps runners and tokens apart.
    pub async fn claim(&self, labels: &[&str], seed: u8) -> Option<rbit_storage::sqlite::actions_runs::Claimed> {
        use rbit_core::actions::{RunnerLabel, RunnerScope};
        use rbit_storage::sqlite::{
            actions_runners::{NewRegistrationToken, NewRunner},
            actions_runs::ClaimRequest,
        };
        let repo = self.repo(&self.alice).await.repo.repo.id;
        let labels: Vec<RunnerLabel> = labels.iter().map(|label| label.parse().unwrap()).collect();
        let runners = self.database.actions_runners();
        let now = Timestamp::now();
        let registration = [seed; 32];
        let token = NewRegistrationToken {
            digest: &registration,
            scope: RunnerScope::Repository(repo),
            labels: None,
            uses: Some(1),
            created_by: Some(self.alice.id()),
            created_at: now,
            expires_at: Timestamp(now.0 + 3600),
        };
        runners.create_registration_token(&token).await.unwrap();
        let uuid = format!("00000000-0000-0000-0000-0000000000{seed:02x}");
        let runner = NewRunner {
            registration_digest: &registration,
            uuid: &uuid,
            token_digest: &[seed.wrapping_add(100); 32],
            name: "runner",
            version: "v12.0.0",
            labels: &labels,
            ephemeral: false,
            now,
        };
        let runner = runners.register(&runner).await.unwrap().unwrap();
        let request = ClaimRequest {
            runner: runner.id,
            scope: runner.scope,
            labels: &labels,
            ephemeral: false,
            instance_runners_for_all: false,
            token_digest: &[seed.wrapping_add(200); 32],
            request_key: None,
            now,
        };
        self.database.actions_runs().claim(&request).await.unwrap()
    }
}
