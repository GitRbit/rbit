//! Archived repositories: who may archive, what is recorded, and the archive guard at
//! every chokepoint that changes a repository — pushes, browser edits, merges, issues,
//! pull requests, comments, reviews, labels, milestones, releases, commit statuses and
//! LFS — while reads, forks and stars keep working.

use std::{path::Path, process::Command, sync::Arc};

use rbit_core::{
    Config, IssueNumber, Permission, RefUpdate, User, Visibility, issue::MergeStrategy, lfs::LfsOperation, pull::HeadSpec,
    repository::ARCHIVED_READ_ONLY,
};
use rbit_services::{
    CreateRepository, DomainEvent, RepoContext, ServiceError, Services,
    forks::ForkInput,
    issues::{LabelInput, MilestoneInput, NewIssueInput},
    lfs::{BatchRequestObject, TokenSubject},
    protection::PushActor,
    pulls::{MergeFailure, MergeInput, NewPullInput, PullContext, ReviewVerdict},
    releases::ReleaseInput,
    statuses::NewStatus,
    timeline::StateChange,
};
use rbit_storage::{Database, KvStore, NewUser, sqlite::audit::AuditAction};

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git is installed");
    assert!(output.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn commit(dir: &Path, file: &str, contents: &str, message: &str) -> String {
    std::fs::write(dir.join(file), contents).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

async fn user(database: &Database, name: &str) -> User {
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

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
    /// The administrator.
    alice: User,
    /// A writer.
    carol: User,
    /// Anyone else.
    dave: User,
    /// The tip of `main`.
    main: String,
}

impl Fixture {
    /// alice/demo (public) with `main` and `feature` (one commit ahead), issue #1 and
    /// pull request #2 from `feature`; carol is a writer.
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let alice = user(&database, "alice").await;
        let carol = user(&database, "carol").await;
        let dave = user(&database, "dave").await;
        let input = CreateRepository { name: "demo".into(), description: String::new(), visibility: Visibility::Public };
        services.repositories.create(&alice, input).await.unwrap();
        let repo = services.repositories.open(Some(&alice), "alice", "demo").await.unwrap();
        services.collaborators.add(&alice, &repo, "carol", Permission::Write).await.unwrap();

        let work = dir.path().join("work");
        std::fs::create_dir(&work).unwrap();
        git(&work, &["init", "-q"]);
        let main = commit(&work, "README.md", "# Demo\n", "Initial commit");
        git(&work, &["push", "-q", repo.disk_path.to_str().unwrap(), "main"]);
        git(&work, &["checkout", "-q", "-b", "feature"]);
        commit(&work, "feature.txt", "one\n", "Add the feature");
        git(&work, &["push", "-q", repo.disk_path.to_str().unwrap(), "feature"]);

        let fixture = Self { _dir: dir, database, services, alice, carol, dave, main };
        let repo = fixture.repo(&fixture.alice).await;
        let issue = NewIssueInput { title: "A bug".into(), body: "It breaks.".into(), ..NewIssueInput::default() };
        fixture.services.issues.create(&fixture.carol, &repo, issue).await.unwrap();
        let head = fixture.services.pulls.resolve_head(Some(&fixture.carol), &repo, &HeadSpec::parse("feature").unwrap()).await.unwrap();
        let pull =
            NewPullInput { base_branch: "main".into(), head_branch: "feature".into(), title: "Feature".into(), ..NewPullInput::default() };
        fixture.services.pulls.create(&fixture.carol, &repo, &head, pull).await.unwrap();
        fixture
    }

    async fn repo(&self, viewer: &User) -> RepoContext {
        self.services.repositories.open(Some(viewer), "alice", "demo").await.unwrap()
    }

    async fn pull(&self, viewer: &User) -> PullContext {
        self.services.pulls.open(Some(viewer), self.repo(viewer).await, IssueNumber::new(2).unwrap()).await.unwrap()
    }

    async fn archive(&self, archived: bool) {
        self.services.repo_settings.set_archived(&self.alice, &self.repo(&self.alice).await, archived).await.unwrap();
    }

    fn merge_input() -> MergeInput {
        MergeInput {
            strategy: MergeStrategy::Merge,
            title: String::new(),
            message: String::new(),
            expected_head: None,
            delete_head_branch: false,
            bypass: false,
        }
    }
}

#[track_caller]
fn archived<T: std::fmt::Debug>(result: &Result<T, ServiceError>) {
    assert!(matches!(result, Err(ServiceError::Archived)), "expected the archive guard, got {result:?}");
}

#[tokio::test]
async fn administrators_archive_and_unarchive_with_an_audit_entry_and_an_event() {
    let fixture = Fixture::new().await;
    let settings = &fixture.services.repo_settings;
    let as_carol = fixture.repo(&fixture.carol).await;
    assert!(matches!(settings.set_archived(&fixture.carol, &as_carol, true).await, Err(ServiceError::Forbidden)), "writers may not");

    let mut events = fixture.services.events.subscribe();
    let as_alice = fixture.repo(&fixture.alice).await;
    assert!(settings.set_archived(&fixture.alice, &as_alice, true).await.unwrap());
    let id = as_alice.repo.repo.id;
    assert_eq!(events.recv().await.unwrap(), DomainEvent::RepositoryArchived { repo: id, archived: true, actor: fixture.alice.id() });
    let reopened = fixture.repo(&fixture.alice).await;
    assert!(reopened.repo.repo.is_archived());
    assert!(!settings.set_archived(&fixture.alice, &reopened, true).await.unwrap(), "already archived: nothing changes");

    assert!(settings.set_archived(&fixture.alice, &reopened, false).await.unwrap());
    assert_eq!(events.recv().await.unwrap(), DomainEvent::RepositoryArchived { repo: id, archived: false, actor: fixture.alice.id() });
    assert!(!fixture.repo(&fixture.alice).await.repo.repo.is_archived());

    let audited = fixture.database.audit().list_for_actor(fixture.alice.id(), 10).await.unwrap();
    let actions: Vec<_> = audited.iter().map(|event| event.action).collect();
    assert!(actions.contains(&AuditAction::RepositoryArchived) && actions.contains(&AuditAction::RepositoryUnarchived), "{actions:?}");
    assert!(audited.iter().all(|event| event.target_name.as_deref() == Some("alice/demo")));
}

#[tokio::test]
async fn archived_repositories_refuse_pushes_and_edits() {
    let fixture = Fixture::new().await;
    fixture.archive(true).await;
    let repo = fixture.repo(&fixture.alice).await;
    let update = RefUpdate { name: "refs/heads/main".into(), before: Some(fixture.main.clone()), after: Some("1".repeat(40)) };
    let created = RefUpdate { name: "refs/heads/new".into(), before: None, after: Some("1".repeat(40)) };
    let actor = PushActor { user: Some(&fixture.alice), deploy_key: false, access: repo.access };
    let verdict = fixture.services.protection.check_push(&repo, &actor, &[update, created]).await.unwrap();
    assert!(verdict.is_refused());
    assert_eq!(verdict.refusals.len(), 2, "every ref is refused, whatever the rules say");
    assert!(verdict.refusals.iter().all(|refusal| refusal.reason == ARCHIVED_READ_ONLY));

    let edits = &fixture.services.edits;
    assert!(!edits.can_edit(&repo).await.unwrap(), "the editor is not offered");
    assert_eq!(edits.read_only_reason(&repo).await.unwrap(), Some(ARCHIVED_READ_ONLY));
    assert!(edits.access(&fixture.alice, &repo, "main").await.is_err());
    assert_eq!(
        fixture.services.protection.branch_update_refusal(&repo, &fixture.alice, "main").await.unwrap().as_deref(),
        Some(ARCHIVED_READ_ONLY)
    );
}

#[tokio::test]
async fn archived_repositories_refuse_issue_and_pull_request_changes() {
    let fixture = Fixture::new().await;
    fixture.archive(true).await;
    let (alice, carol, dave) = (&fixture.alice, &fixture.carol, &fixture.dave);
    let repo = fixture.repo(carol).await;
    let issues = &fixture.services.issues;
    let timeline = &fixture.services.timeline;

    let new = || NewIssueInput { title: "Another".into(), body: String::new(), ..NewIssueInput::default() };
    archived(&issues.create(dave, &fixture.repo(dave).await, new()).await);
    archived(&issues.create(carol, &repo, new()).await);
    let issue = timeline.open(Some(carol), repo.clone(), IssueNumber::new(1).unwrap()).await.unwrap();
    assert_eq!(issue.permissions, rbit_core::IssuePermissions::default(), "nothing is offered on the page");
    archived(&timeline.comment(carol, &issue, "Still broken").await);
    archived(&timeline.edit(carol, &issue, Some("Renamed"), None).await);
    archived(&timeline.set_state(carol, &issue, StateChange::Close(rbit_core::StateReason::Completed)).await);
    archived(&timeline.set_locked(carol, &issue, true).await);
    let as_dave = timeline.open(Some(dave), fixture.repo(dave).await, IssueNumber::new(1).unwrap()).await.unwrap();
    archived(&timeline.comment(dave, &as_dave, "Me too").await);
    archived(&issues.change_labels(carol, &issue, &[], &[]).await);
    archived(&issues.set_milestone(carol, &issue, None).await);
    archived(
        &issues.delete(alice, &timeline.open(Some(alice), fixture.repo(alice).await, IssueNumber::new(1).unwrap()).await.unwrap()).await,
    );

    let label = LabelInput { name: "bug".into(), color: "#d73a4a".into(), description: String::new() };
    archived(&issues.create_label(carol, &repo, &label).await);
    archived(&issues.create_default_labels(carol, &repo).await);
    let milestone = MilestoneInput { title: "v1".into(), ..MilestoneInput::default() };
    archived(&issues.create_milestone(carol, &repo, &milestone).await);

    let pulls = &fixture.services.pulls;
    let head = pulls.resolve_head(Some(carol), &repo, &HeadSpec::parse("feature").unwrap()).await.unwrap();
    let input =
        NewPullInput { base_branch: "main".into(), head_branch: "feature".into(), title: "Again".into(), ..NewPullInput::default() };
    archived(&pulls.create(carol, &repo, &head, input).await);
    let pull = fixture.pull(alice).await;
    assert!(matches!(pulls.merge(alice, &pull, Fixture::merge_input()).await, Err(MergeFailure::Service(ServiceError::Archived))));
    assert!(!pulls.merge_box(Some(alice), &pull).await.unwrap().can_merge, "the merge button is not offered");
    archived(&pulls.set_draft(carol, &fixture.pull(carol).await, true).await);
    archived(&pulls.submit_review(alice, &pull, ReviewVerdict::Approve, "").await);
    archived(&pulls.request_reviewers(carol, &fixture.pull(carol).await, &["alice".to_owned()]).await);
}

#[tokio::test]
async fn archived_repositories_refuse_releases_statuses_and_lfs_writes() {
    let fixture = Fixture::new().await;
    fixture.archive(true).await;
    let repo = fixture.repo(&fixture.carol).await;

    let release = ReleaseInput { tag_name: "v1".into(), target: "main".into(), ..ReleaseInput::default() };
    archived(&fixture.services.releases.create(&repo, &fixture.carol, release).await);
    let status = NewStatus { state: "success".into(), target_url: None, description: None, context: Some("ci".into()) };
    archived(&fixture.services.statuses.create(&fixture.carol, &repo, &fixture.main, &status).await);

    let lfs = &fixture.services.lfs;
    let object = BatchRequestObject { oid: "a".repeat(64), size: 1 };
    let subject = TokenSubject::User(fixture.carol.id());
    archived(&lfs.batch(&repo, Some(&fixture.carol), subject, LfsOperation::Upload, std::slice::from_ref(&object)).await);
    assert!(lfs.batch(&repo, Some(&fixture.carol), subject, LfsOperation::Download, &[object]).await.is_ok(), "downloads keep working");
    archived(&lfs.create_lock(&repo, Some(&fixture.carol), "assets/logo.psd").await);
}

#[tokio::test]
async fn archived_repositories_stay_readable_forkable_and_starrable() {
    let fixture = Fixture::new().await;
    fixture.archive(true).await;
    let dave = &fixture.dave;
    let repo = fixture.repo(dave).await;
    assert!(!repo.can_write_content());

    let thread = fixture.services.timeline.open(Some(dave), repo.clone(), IssueNumber::new(1).unwrap()).await.unwrap();
    assert_eq!(thread.issue.title.as_str(), "A bug");
    fixture.services.timeline.timeline(Some(dave), &thread, None, 50).await.unwrap();
    fixture.services.stars.star(dave, &repo).await.unwrap();
    let fork = fixture.services.forks.fork(dave, &repo, ForkInput::default()).await.unwrap();
    assert!(!fork.repo.is_archived(), "a fork of an archived repository accepts changes");
    let fork = fixture.services.repositories.open(Some(dave), "dave", "demo").await.unwrap();
    let issue = NewIssueInput { title: "In my fork".into(), body: String::new(), ..NewIssueInput::default() };
    fixture.services.issues.create(dave, &fork, issue).await.unwrap();
}

#[tokio::test]
async fn unarchiving_restores_writes() {
    let fixture = Fixture::new().await;
    fixture.archive(true).await;
    fixture.archive(false).await;
    let carol = &fixture.carol;
    let repo = fixture.repo(carol).await;
    let issue = fixture.services.timeline.open(Some(carol), repo, IssueNumber::new(1).unwrap()).await.unwrap();
    assert!(issue.permissions.can_comment);
    fixture.services.timeline.comment(carol, &issue, "Back again").await.unwrap();
    let alice = &fixture.alice;
    let pull = fixture.pull(alice).await;
    fixture.services.pulls.merge(alice, &pull, Fixture::merge_input()).await.unwrap();
}
