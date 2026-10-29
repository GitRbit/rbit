//! Commit statuses: who may post them, validation, reading them back by branch or id,
//! the domain event and the `status` webhook payload, against real stores and a bare
//! repository filled with the `git` CLI.

use std::{path::Path, process::Command, sync::Arc};

use rbit_core::{
    Config, User, Visibility,
    membership::Permission,
    status::{StatusId, StatusReport, StatusSource, StatusState},
    webhook::WebhookEvent,
};
use rbit_services::{
    CreateRepository, DomainEvent, RepoContext, ServiceError, Services,
    statuses::NewStatus,
    webhooks::{HookTarget, WebhookInput},
};
use rbit_storage::{Database, KvStore, NewUser, sqlite::webhooks::SecretUpdate};
use serde_json::Value;

struct Fixture {
    _dir: tempfile::TempDir,
    services: Services,
    database: Database,
    /// Owns `alice/rocket` (private).
    alice: User,
    /// A collaborator with read access.
    rita: User,
    /// A collaborator with write access (the CI bot).
    carol: User,
    /// The tip of `main` (`Second`) and its parent (`First`).
    head: String,
    first: String,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        "https://git.example.com".clone_into(&mut config.server.public_url);
        config.webhooks.allow_private_addresses = true;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let user = |name: &'static str| {
            let database = database.clone();
            async move {
                let new = NewUser {
                    name: name.parse().unwrap(),
                    email: format!("{name}@example.com").parse().unwrap(),
                    display_name: None,
                    password_hash: "not-a-real-hash".to_owned(),
                    is_admin: false,
                };
                database.accounts().create_user(new).await.unwrap()
            }
        };
        let (alice, rita, carol) = (user("alice").await, user("rita").await, user("carol").await);
        let input = CreateRepository { name: "rocket".into(), description: String::new(), visibility: Visibility::Private };
        services.repositories.create(&alice, input).await.unwrap();
        let repo = services.repositories.open(Some(&alice), "alice", "rocket").await.unwrap();
        services.collaborators.add(&alice, &repo, "rita", Permission::Read).await.unwrap();
        services.collaborators.add(&alice, &repo, "carol", Permission::Write).await.unwrap();
        let (first, head) = push_two_commits(&repo);
        Self { _dir: dir, services, database, alice, rita, carol, head, first }
    }

    async fn repo(&self, viewer: &User) -> RepoContext {
        self.services.repositories.open(Some(viewer), "alice", "rocket").await.unwrap()
    }

    async fn post(
        &self,
        user: &User,
        sha: &str,
        state: &str,
        context: Option<&str>,
    ) -> Result<rbit_core::status::CommitStatus, ServiceError> {
        let input = NewStatus {
            state: state.to_owned(),
            context: context.map(str::to_owned),
            description: Some("Build finished".to_owned()),
            target_url: Some("https://ci.example.com/builds/7".to_owned()),
        };
        self.services.statuses.create(user, &self.repo(user).await, sha, &input).await
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Pushes two commits to `main` of `repo`; returns (first, second).
fn push_two_commits(repo: &RepoContext) -> (String, String) {
    let work = tempfile::tempdir().unwrap();
    git(work.path(), &["init", "-q"]);
    std::fs::write(work.path().join("README.md"), "# Demo\n").unwrap();
    git(work.path(), &["add", "."]);
    git(work.path(), &["commit", "-q", "-m", "First"]);
    let first = git(work.path(), &["rev-parse", "HEAD"]);
    std::fs::write(work.path().join("main.rs"), "fn main() {}\n").unwrap();
    git(work.path(), &["add", "."]);
    git(work.path(), &["commit", "-q", "-m", "Second", "-m", "With a body."]);
    git(work.path(), &["push", "-q", repo.disk_path.to_str().unwrap(), "HEAD:refs/heads/main"]);
    (first, git(work.path(), &["rev-parse", "HEAD"]))
}

fn invalid<T: std::fmt::Debug>(result: Result<T, ServiceError>) -> String {
    match result {
        Err(ServiceError::Invalid(message)) => message,
        other => panic!("expected a validation error, got {other:?}"),
    }
}

#[tokio::test]
async fn writers_post_statuses_and_readers_read_them() {
    let fixture = Fixture::new().await;
    let statuses = &fixture.services.statuses;
    let mut events = fixture.services.events.subscribe();

    assert!(matches!(fixture.post(&fixture.rita, &fixture.head, "success", None).await, Err(ServiceError::Forbidden)));
    let pending = fixture.post(&fixture.carol, &fixture.head, "pending", Some("ci/build")).await.unwrap();
    assert_eq!((pending.state, pending.context.as_str(), pending.creator), (StatusState::Pending, "ci/build", Some(fixture.carol.id())));
    match events.recv().await.unwrap() {
        DomainEvent::CommitStatus { status, sha, actor, .. } => {
            assert_eq!((status, sha, actor), (pending.id, fixture.head.clone(), Some(fixture.carol.id())));
        }
        other => panic!("expected the status event, got {other:?}"),
    }
    let upper = fixture.head.to_ascii_uppercase();
    fixture.post(&fixture.carol, &upper, "success", Some("ci/build")).await.unwrap();
    fixture.post(&fixture.alice, &fixture.head, "failure", None).await.unwrap();
    fixture.post(&fixture.alice, &fixture.first, "success", Some("ci/build")).await.unwrap();

    // Validation.
    assert_eq!(
        invalid(fixture.post(&fixture.carol, &fixture.head, "done", None).await),
        "state must be one of pending, success, failure or error"
    );
    assert_eq!(invalid(fixture.post(&fixture.carol, &fixture.head[..7], "success", None).await), "sha must be a full commit id");
    let missing = "0123456789".repeat(4);
    assert_eq!(invalid(fixture.post(&fixture.carol, &missing, "success", None).await), format!("no commit found for SHA {missing}"));
    let input = NewStatus { state: "success".into(), target_url: Some("javascript:alert(1)".into()), ..NewStatus::default() };
    let repo = fixture.repo(&fixture.carol).await;
    assert!(invalid(statuses.create(&fixture.carol, &repo, &fixture.head, &input).await).starts_with("target_url must be"));

    // Reading by branch, by id.
    let reader = fixture.repo(&fixture.rita).await;
    let page = statuses.list(&reader, "main", 0, 10).await.unwrap();
    assert_eq!((page.sha.as_str(), page.total), (fixture.head.as_str(), 3));
    let states: Vec<_> = page.statuses.iter().map(|status| (status.context.as_str(), status.state)).collect();
    assert_eq!(states, [("default", StatusState::Failure), ("ci/build", StatusState::Success), ("ci/build", StatusState::Pending)]);
    assert_eq!(statuses.list(&reader, "main", 2, 10).await.unwrap().statuses.len(), 1);

    let combined = statuses.combined(&reader, &fixture.head[..10]).await.unwrap();
    assert_eq!(combined.state, StatusState::Failure, "the default context failed");
    let latest: Vec<_> = combined.statuses.iter().map(|status| (status.context.as_str(), status.state)).collect();
    assert_eq!(latest, [("ci/build", StatusState::Success), ("default", StatusState::Failure)]);
    assert_eq!(statuses.combined(&reader, &fixture.first).await.unwrap().state, StatusState::Success);
    assert!(matches!(statuses.combined(&reader, "no-such-branch").await, Err(ServiceError::NotFound)));

    let many = statuses.combined_of(&reader, &[fixture.head.clone(), fixture.first.clone(), missing]).await.unwrap();
    assert_eq!(many.len(), 2, "commits without statuses are absent");
    assert_eq!(many[&fixture.first].state, StatusState::Success);
    let creators = statuses.creators(&page.statuses).await.unwrap();
    let names: Vec<_> = creators.iter().map(|account| account.name.as_str()).collect();
    assert_eq!(names, ["alice", "carol"]);
}

#[tokio::test]
async fn the_status_webhook_has_githubs_shape() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let webhooks = &fixture.services.webhooks;
    let input = WebhookInput {
        url: "http://127.0.0.1:9/hook".to_owned(),
        events: vec!["status".to_owned()],
        active: true,
        secret: SecretUpdate::Keep,
    };
    let hook = webhooks.create(&fixture.alice, HookTarget::Repository(&repo), input).await.unwrap();
    assert_eq!(hook.events, [WebhookEvent::Status]);

    let status = fixture.post(&fixture.carol, &fixture.head, "success", Some("ci/build")).await.unwrap();
    let event =
        DomainEvent::CommitStatus { repo: repo.repo.repo.id, status: status.id, sha: status.sha.clone(), actor: Some(fixture.carol.id()) };
    assert_eq!(webhooks.handle_event(&event).await.unwrap(), 1);
    let gone = DomainEvent::CommitStatus {
        repo: repo.repo.repo.id,
        status: StatusId(999),
        sha: status.sha.clone(),
        actor: Some(fixture.carol.id()),
    };
    assert_eq!(webhooks.handle_event(&gone).await.unwrap(), 0, "a status that no longer exists sends nothing");

    let log = webhooks.deliveries(HookTarget::Repository(&repo), hook.id, 1, 10).await.unwrap();
    let delivery = log.deliveries.iter().find(|delivery| delivery.event == WebhookEvent::Status).unwrap();
    assert_eq!(delivery.action, None, "status payloads have no action");
    let payload: Value = serde_json::from_str(&delivery.payload).unwrap();
    assert_eq!(payload["id"], status.id.0);
    assert_eq!(payload["sha"], fixture.head.as_str());
    assert_eq!(payload["name"], "alice/rocket");
    assert_eq!((payload["state"].as_str(), payload["context"].as_str()), (Some("success"), Some("ci/build")));
    assert_eq!(payload["description"], "Build finished");
    assert_eq!(payload["target_url"], "https://ci.example.com/builds/7");
    assert_eq!(payload["commit"]["sha"], fixture.head.as_str());
    assert_eq!(payload["commit"]["commit"]["message"], "Second\n\nWith a body.");
    assert_eq!(payload["commit"]["html_url"], format!("https://git.example.com/alice/rocket/commit/{}", fixture.head));
    assert_eq!(payload["commit"]["parents"][0]["sha"], fixture.first.as_str());
    assert_eq!(payload["branches"][0]["name"], "main");
    assert_eq!(payload["branches"][0]["protected"], false);
    assert_eq!(payload["repository"]["full_name"], "alice/rocket");
    assert_eq!(payload["sender"]["login"], "carol");
    assert!(payload["created_at"].as_str().is_some_and(|at| at.ends_with('Z')));
    assert!(payload.get("organization").is_none(), "a personal repository");
}

// Actions: triggers (T).
#[tokio::test]
async fn statuses_from_actions_name_the_bot_as_creator_and_sender() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let webhooks = &fixture.services.webhooks;
    let input = WebhookInput {
        url: "http://127.0.0.1:9/hook".to_owned(),
        events: vec!["status".to_owned()],
        active: true,
        secret: SecretUpdate::Keep,
    };
    let hook = webhooks.create(&fixture.alice, HookTarget::Repository(&repo), input).await.unwrap();
    let report = StatusReport::parse("pending", Some("CI / build (push)"), Some("Queued"), None).unwrap();
    let status =
        fixture.database.commit_statuses().create_as_system(repo.repo.repo.id, &fixture.head, &report, 1000).await.unwrap().unwrap();
    assert_eq!((status.creator, status.source), (None, StatusSource::Actions));

    let event = DomainEvent::CommitStatus { repo: repo.repo.repo.id, status: status.id, sha: status.sha.clone(), actor: None };
    assert_eq!(webhooks.handle_event(&event).await.unwrap(), 1);
    let log = webhooks.deliveries(HookTarget::Repository(&repo), hook.id, 1, 10).await.unwrap();
    let payload: Value = serde_json::from_str(&log.deliveries[0].payload).unwrap();
    assert_eq!(payload["sender"]["login"], "rbit-actions[bot]");
    assert_eq!((payload["sender"]["type"].as_str(), payload["sender"]["id"].as_i64()), (Some("Bot"), Some(0)));

    let json = rbit_services::statuses::hooks::status(&status, None, &repo.repo, "https://git.example.com");
    assert_eq!(json["creator"]["login"], "rbit-actions[bot]");
    let person = fixture.post(&fixture.carol, &fixture.head, "success", None).await.unwrap();
    assert_eq!(rbit_services::statuses::hooks::status(&person, None, &repo.repo, "https://git.example.com")["creator"], Value::Null);
    assert!(fixture.services.statuses.creators(&[status]).await.unwrap().is_empty(), "the bot is no account");
}
