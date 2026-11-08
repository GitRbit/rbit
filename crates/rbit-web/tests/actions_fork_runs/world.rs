//! The fork-run world: `alice/rocket` (private, Actions on) with a workflow, secrets and
//! variables at every scope a job could receive them from, a reader `bob` who forks it and
//! opens pull requests, a writer `carol` who approves them, and one runner.

use std::{path::PathBuf, process::Command};

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use rbit_core::{
    IssueId, Permission, User,
    actions::{Run, SecretVisibility},
    issue::IssueAction,
    pull::HeadSpec,
};
use rbit_services::{
    RepoContext,
    actions::{environments::EnvironmentInput, secrets::ActionsTarget},
    forks::ForkInput,
    pulls::NewPullInput,
};
use rbit_storage::NewUser;
use serde_json::Value;
use tower::ServiceExt;

use crate::actions_runner::Fixture;

/// Every value a trusted job of the workflow may receive; none may reach a fork's job.
pub const SECRET_VALUES: [&str; 6] = [
    "repo-secret-4f1c",
    "owner-secret-9a2e",
    "environment-secret-7d3b",
    "repo-variable-c81e",
    "owner-variable-2b6f",
    "environment-variable-e5a0",
];

/// `build` asks for every write permission, `deploy` runs in `production`, and `gated` runs
/// only when a variable says so — which a fork's run must never see.
pub const WORKFLOW: &str = "name: CI\non: [push, pull_request]\njobs:\n  build:\n    runs-on: linux\n    permissions: write-all\n    steps:\n      - run: echo build ${{ secrets.REPO_SECRET }}\n  deploy:\n    runs-on: linux\n    environment: production\n    steps:\n      - run: echo deploy\n  gated:\n    runs-on: linux\n    if: vars.GATE == 'open'\n    steps:\n      - run: echo gated\n";

pub struct World {
    pub fixture: Fixture,
    /// Reads `alice/rocket`; owns the fork `bob/rocket`.
    pub bob: User,
    /// Writes `alice/rocket`.
    pub carol: User,
    /// A runner of `alice/rocket` for `linux`.
    pub runner: String,
    work: PathBuf,
    _work: tempfile::TempDir,
}

pub fn git(dir: &std::path::Path, args: &[&str]) -> String {
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

async fn user(fixture: &Fixture, name: &str) -> User {
    let new = NewUser {
        name: name.parse().unwrap(),
        email: format!("{name}@example.com").parse().unwrap(),
        display_name: None,
        password_hash: "not-a-real-hash".to_owned(),
        is_admin: false,
    };
    fixture.database.accounts().create_user(new).await.unwrap()
}

impl World {
    pub async fn new() -> Self {
        let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
        let (bob, carol) = (user(&fixture, "bob").await, user(&fixture, "carol").await);
        let repo = fixture.repo().await;
        let collaborators = &fixture.services.collaborators;
        collaborators.add(&fixture.alice, &repo, "bob", Permission::Read).await.unwrap();
        collaborators.add(&fixture.alice, &repo, "carol", Permission::Write).await.unwrap();
        let runner = fixture.runner(&["linux"]).await;
        let work = tempfile::tempdir().unwrap();
        let world = Self { fixture, bob, carol, runner, work: work.path().to_owned(), _work: work };
        git(&world.work, &["init", "-q"]);
        world.store_values().await;
        world.commit(".rbit/workflows/ci.yml", WORKFLOW, "CI");
        world.push(&world.fixture.repo().await, "refs/heads/main");
        let parent = world.repo(&world.bob).await;
        world.fixture.services.forks.fork(&world.bob, &parent, ForkInput::default()).await.unwrap();
        world
    }

    /// Secrets and variables of the repository, its owner and its `production` environment.
    async fn store_values(&self) {
        let (actions, alice) = (self.fixture.actions(), &self.fixture.alice);
        let repo = self.fixture.repo().await;
        let [repo_secret, owner_secret, environment_secret, repo_variable, owner_variable, environment_variable] = SECRET_VALUES;
        actions.put_secret(alice, ActionsTarget::Repository(&repo), "REPO_SECRET", repo_secret, SecretVisibility::All).await.unwrap();
        actions.put_secret(alice, ActionsTarget::User(alice), "OWNER_SECRET", owner_secret, SecretVisibility::All).await.unwrap();
        actions.put_variable(alice, ActionsTarget::Repository(&repo), "REPO_VARIABLE", repo_variable).await.unwrap();
        actions.put_variable(alice, ActionsTarget::Repository(&repo), "GATE", "open").await.unwrap();
        actions.put_variable(alice, ActionsTarget::User(alice), "OWNER_VARIABLE", owner_variable).await.unwrap();
        let environment = actions.put_environment(alice, &repo, "production", &EnvironmentInput::default()).await.unwrap();
        let target = ActionsTarget::Environment(&repo, &environment);
        actions.put_secret(alice, target, "ENVIRONMENT_SECRET", environment_secret, SecretVisibility::All).await.unwrap();
        actions.put_variable(alice, target, "ENVIRONMENT_VARIABLE", environment_variable).await.unwrap();
    }

    pub async fn repo(&self, viewer: &User) -> RepoContext {
        self.fixture.services.repositories.open(Some(viewer), "alice", "rocket").await.unwrap()
    }

    /// Writes `path` in the work tree and commits it; returns the commit.
    pub fn commit(&self, path: &str, contents: &str, message: &str) -> String {
        let file = self.work.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
        git(&self.work, &["add", "-A"]);
        git(&self.work, &["commit", "-q", "-m", message]);
        git(&self.work, &["rev-parse", "HEAD"])
    }

    /// Force-pushes `HEAD` to `reference` of `repo`.
    pub fn push(&self, repo: &RepoContext, reference: &str) {
        git(&self.work, &["push", "-q", "-f", repo.disk_path.to_str().unwrap(), &format!("HEAD:{reference}")]);
    }

    /// Pushes `HEAD` to `main` of `alice/rocket` as alice: a trusted run.
    pub async fn push_run(&self) -> Run {
        let repo = self.fixture.repo().await;
        let before = repo.reader.commit_id("refs/heads/main".to_owned()).await.ok();
        self.push(&repo, "refs/heads/main");
        let after = Some(git(&self.work, &["rev-parse", "HEAD"]));
        let update = rbit_core::RefUpdate { name: "refs/heads/main".to_owned(), before, after };
        let source = rbit_core::PushSource::User(self.fixture.alice.id());
        self.fixture.actions().handle_push(repo.repo.repo.id, source, &[update]).await.unwrap().remove(0)
    }

    /// Pushes `HEAD` to bob's `fix` branch and opens a pull request of it; its run.
    pub async fn open_pull(&self) -> (IssueId, Run) {
        let fork = self.fixture.services.repositories.open(Some(&self.bob), "bob", "rocket").await.unwrap();
        self.push(&fork, "refs/heads/fix");
        let base = self.repo(&self.bob).await;
        let pulls = &self.fixture.services.pulls;
        let head = pulls.resolve_head(Some(&self.bob), &base, &HeadSpec::parse("bob:fix").unwrap()).await.unwrap();
        let input = NewPullInput { base_branch: "main".into(), head_branch: "fix".into(), title: "Fix".into(), ..NewPullInput::default() };
        let issue = pulls.create(&self.bob, &base, &head, input).await.unwrap().0.id;
        let run = self.pull_event(issue, &IssueAction::Opened).await;
        (issue, run)
    }

    /// Bob pushes `HEAD` to his `fix` branch again; the `synchronize` run.
    pub async fn synchronize(&self, issue: IssueId) -> Run {
        let fork = self.fixture.services.repositories.open(Some(&self.bob), "bob", "rocket").await.unwrap();
        let before = fork.reader.commit_id("refs/heads/fix".to_owned()).await.unwrap();
        self.push(&fork, "refs/heads/fix");
        let after = git(&self.work, &["rev-parse", "HEAD"]);
        let source = rbit_core::PushSource::User(self.bob.id());
        let update = rbit_core::RefUpdate { name: "refs/heads/fix".to_owned(), before: Some(before.clone()), after: Some(after.clone()) };
        self.fixture.services.pulls.handle_push(fork.repo.repo.id, source, &[update]).await.unwrap();
        self.pull_event(issue, &IssueAction::Synchronized { before, after, source }).await
    }

    async fn pull_event(&self, issue: IssueId, action: &IssueAction) -> Run {
        let repo = self.fixture.repo().await.repo.repo.id;
        let mut runs = self.fixture.actions().handle_pull_request(repo, issue, Some(self.bob.id()), action).await.unwrap();
        assert_eq!(runs.len(), 1, "{runs:?}");
        runs.remove(0)
    }

    /// Carol approves `run` through the run page's form.
    pub async fn approve_on_page(&self, run: &Run, trust_author: bool) -> StatusCode {
        let session = self.fixture.services.accounts.start_session(&self.carol).await.unwrap().encode();
        let body = if trust_author { "trust_author=true" } else { "" };
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/alice/rocket/actions/runs/{}/approve", run.number))
            .header(header::COOKIE, format!("rbit_session={session}"))
            .header(header::HOST, "localhost:3000")
            .header(header::ORIGIN, "http://localhost:3000")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body))
            .unwrap();
        self.fixture.router.clone().oneshot(request).await.unwrap().status()
    }

    /// A poll with idempotency key `key`: the raw answer, so nothing in it escapes a check.
    pub async fn poll_raw(&self, key: &str) -> (StatusCode, String) {
        let headers = [("rbit-runner-protocol", "1"), ("idempotency-key", key)];
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/actions/runner/v1/poll")
            .header(header::HOST, "git.example.com")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {}", self.runner));
        let request = headers.iter().fold(request, |request, (name, value)| request.header(*name, *value));
        let response = self.fixture.router.clone().oneshot(request.body(Body::from(r#"{"free_slots":1}"#)).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    /// Polls until nothing is left: the claimed jobs, sorted by job id.
    pub async fn claim_all(&self, prefix: &str) -> Vec<Claimed> {
        let mut claimed = Vec::new();
        for attempt in 0..16 {
            let (status, raw) = self.poll_raw(&format!("{prefix}-{attempt}")).await;
            if status == StatusCode::NO_CONTENT {
                claimed.sort_by(|a: &Claimed, b| a.job.cmp(&b.job));
                return claimed;
            }
            assert_eq!(status, StatusCode::OK, "{raw}");
            let json: Value = serde_json::from_str(&raw).unwrap();
            let job = json["task"]["template"]["job_id"].as_str().unwrap().to_owned();
            claimed.push(Claimed { job, json, raw });
        }
        panic!("more jobs than the workflow has");
    }
}

/// A claimed job: its id, its specification and the poll's answer as sent.
pub struct Claimed {
    pub job: String,
    pub json: Value,
    pub raw: String,
}

impl Claimed {
    /// The secret and variable values the answer carries anywhere in it.
    pub fn leaked(&self) -> Vec<&'static str> {
        SECRET_VALUES.into_iter().filter(|value| self.raw.contains(value)).collect()
    }

    pub fn token(&self) -> &str {
        self.json["task"]["token"].as_str().unwrap()
    }
}
