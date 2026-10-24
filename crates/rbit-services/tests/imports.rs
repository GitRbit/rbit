//! Importing from a fake GitHub (and Gitea) server: the Git copy with pull request heads,
//! LFS objects, labels, milestones, issues and pull requests with preserved numbers and
//! attribution, releases with files — against real stores and a temporary data directory.

use std::{sync::Arc, time::Duration};

use rbit_core::{
    Config, IssueKind, IssueNumber, IssueState, IssueText, IssueTitle, StateReason, Timestamp, User, Visibility,
    config::AuthorMapping,
    import::{ImportOptions, ImportPhase, ImportStatus, RepoImport, SourceKind},
    lfs::LfsOid,
};
use rbit_services::{DomainEvent, RepoContext, ServiceError, Services, imports::ImportInput};
use rbit_storage::{Database, NewUser, sqlite::issues::NewIssue, sqlite::issues::StoredTimelineItem};
use tokio::io::AsyncReadExt;

use crate::fake_forge::{FakeForge, LFS_CONTENT, RELEASE_FILE};

const TOKEN: &str = "ghp_secret123";

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
}

impl Fixture {
    async fn new(configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.updates.check_for_updates = false;
        config.storage.data_dir = dir.path().join("data");
        config.imports.allow_private_addresses = true;
        configure(&mut config);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = rbit_storage::KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        Self { _dir: dir, database, services }
    }

    async fn user(&self, name: &str) -> User {
        self.database
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

    async fn open(&self, viewer: &User, name: &str) -> RepoContext {
        self.services.repositories.open(Some(viewer), viewer.name().as_str(), name).await.unwrap()
    }

    /// Waits until the import of `repo` has ended.
    async fn finished(&self, viewer: &User, repo: &RepoContext) -> RepoImport {
        for _ in 0..600 {
            let import = self.services.imports.status(viewer, repo).await.unwrap();
            if !import.status.is_active() {
                return import;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the import never finished");
    }

    async fn thread(&self, repo: &RepoContext, number: u32) -> rbit_core::Issue {
        self.database.issues().find_by_number(repo.repo.repo.id, IssueNumber::new(number).unwrap()).await.unwrap().unwrap()
    }

    async fn comments(&self, issue: &rbit_core::Issue) -> Vec<rbit_core::Comment> {
        let page = self.database.issues().timeline(issue.id, None, 100).await.unwrap();
        page.items
            .into_iter()
            .filter_map(|item| match item {
                StoredTimelineItem::Comment { comment, .. } => Some(comment),
                StoredTimelineItem::Event { .. } => None,
            })
            .collect()
    }
}

fn input(forge: &FakeForge, kind: SourceKind, name: &str) -> ImportInput {
    ImportInput {
        kind,
        host: forge.base.clone(),
        repository: "acme/rocket".to_owned(),
        token: forge.token.clone(),
        owner: String::new(),
        name: name.to_owned(),
        visibility: Visibility::Private,
        options: ImportOptions::default(),
    }
}

fn at(text: &str) -> Timestamp {
    rbit_core::import::parse_timestamp(text).unwrap()
}

#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one import, checked part by part")]
async fn a_github_import_brings_history_with_numbers_and_attribution() {
    let forge = FakeForge::start(TOKEN, None).await;
    let fx = Fixture::new(|_| {}).await;
    let alice = fx.user("alice").await;
    let mut events = fx.services.events.subscribe();

    let created = fx.services.imports.start(&alice, input(&forge, SourceKind::GitHubEnterprise, "")).await.unwrap();
    assert_eq!(created.full_name(), "alice/rocket", "the source's name by default");
    assert_eq!(created.repo.description.as_deref(), Some("To the moon"));
    let repo = fx.open(&alice, "rocket").await;
    let import = fx.finished(&alice, &repo).await;
    assert_eq!((import.status, import.phase, import.error.as_deref()), (ImportStatus::Succeeded, ImportPhase::Finished, None));
    assert_eq!(import.warnings, Vec::<String>::new());
    let counts = import.counts;
    assert_eq!((counts.labels, counts.milestones, counts.issues, counts.pulls), (3, 2, 3, 4));
    assert_eq!((counts.comments, counts.releases, counts.assets, counts.lfs_objects), (4, 2, 1, 1));
    assert_eq!(import.source_url, format!("{}/acme/rocket", forge.base));

    // Git: branches, tags and pull request heads (not refs/pull/*/merge), main as default.
    let repo = fx.open(&alice, "rocket").await;
    assert_eq!(repo.repo.repo.default_branch, "main");
    let reader = &repo.reader;
    for name in ["refs/heads/main", "refs/heads/feature", "refs/tags/v1.0", "refs/pull/1/head", "refs/pull/2/head", "refs/pull/4/head"] {
        assert!(reader.commit_id(name.to_owned()).await.is_ok(), "{name}");
    }
    assert!(reader.commit_id("refs/pull/4/merge".to_owned()).await.is_err());
    assert!(repo.repo.repo.pushed_at.is_some());

    // Issues keep their numbers, states, times and authorship.
    let crash = fx.thread(&repo, 3).await;
    assert_eq!((crash.kind, crash.state, crash.state_reason), (IssueKind::Issue, IssueState::Closed, Some(StateReason::NotPlanned)));
    assert_eq!(
        (crash.created_at, crash.closed_at, crash.locked, crash.comment_count),
        (at("2024-01-03T10:00:00Z"), Some(at("2024-02-03T09:00:00Z")), true, 2)
    );
    assert_eq!(crash.author, Some(alice.id()), "unmapped authors: the importer writes, naming them");
    assert!(crash.body.as_str().starts_with(&format!("*Originally by [@octocat]({}/octocat) on GitHub.*\n\nBody of #3", forge.base)));
    let comments = fx.comments(&crash).await;
    assert_eq!(comments.len(), 2);
    assert!(comments[0].body.as_str().contains("[@hubot]") && comments[0].body.as_str().ends_with("Same here"));
    assert_eq!(comments[0].created_at, at("2024-01-21T10:00:00Z"));
    let meta = fx.database.issue_meta();
    let labels = meta.labels_of(&[crash.id]).await.unwrap();
    assert_eq!(labels.iter().map(|(_, l)| l.name.as_str()).collect::<Vec<_>>(), ["bug"]);
    assert_eq!(labels[0].1.color, "#d73a4a");
    let milestones = meta.milestones_of(&[crash.id]).await.unwrap();
    assert_eq!(milestones.len(), 1);
    let ghost = fx.thread(&repo, 8).await;
    assert!(ghost.body.as_str().starts_with("*Originally by [@ghost]"), "a deleted source account");
    assert_eq!(fx.thread(&repo, 6).await.state, IssueState::Open);
    let none: Vec<_> = meta.assignees_of(&[fx.thread(&repo, 6).await.id]).await.unwrap();
    assert_eq!(none.len(), 0, "assignees need a mapped account");
    assert!(fx.database.issues().find_by_number(repo.repo.repo.id, IssueNumber::new(7).unwrap()).await.unwrap().is_none());

    // Pull requests: merged history, live open ones, and one whose head was lost.
    let pulls = fx.database.pulls();
    let merged = fx.thread(&repo, 1).await;
    let row = pulls.find(merged.id).await.unwrap().unwrap();
    assert!(row.is_merged() && row.merge_strategy.is_none(), "merged outside rbit");
    assert_eq!((merged.state_reason, row.merged_at), (Some(StateReason::Merged), Some(at("2024-02-01T09:00:00Z"))));
    assert_eq!(fx.comments(&merged).await.len(), 1, "pull request comments come along");
    let open = fx.thread(&repo, 2).await;
    let row = pulls.find(open.id).await.unwrap().unwrap();
    assert_eq!(
        (open.state, row.head_repo_id, row.head_branch.as_str(), row.is_draft),
        (IssueState::Open, Some(repo.repo.repo.id), "feature", true)
    );
    assert_eq!(Some(row.head_sha), reader.commit_id("refs/pull/2/head".to_owned()).await.ok());
    let from_fork = pulls.find(fx.thread(&repo, 4).await.id).await.unwrap().unwrap();
    assert_eq!((fx.thread(&repo, 4).await.state, from_fork.head_repo_id), (IssueState::Open, None), "fork heads live in refs/pull/4/head");
    let lost = fx.thread(&repo, 5).await;
    assert_eq!((lost.state, lost.state_reason), (IssueState::Closed, Some(StateReason::NotPlanned)));
    assert!(lost.body.as_str().contains("imported closed"));

    // New threads are numbered after the source's highest.
    let new = NewIssue {
        repo: repo.repo.repo.id,
        kind: IssueKind::Issue,
        title: IssueTitle::parse("After the import").unwrap(),
        body: IssueText::default(),
        author: alice.id(),
        at: Timestamp::now(),
    };
    assert_eq!(fx.database.issues().create(&new).await.unwrap().number.get(), 9);

    // Releases keep their dates; the file came through the redirect intact.
    let releases = fx.database.releases();
    let one = releases.find_by_tag(repo.repo.repo.id, "v1.0").await.unwrap().unwrap();
    assert_eq!((one.name.as_str(), one.is_draft, one.published_at), ("One", false, Some(at("2024-02-11T00:00:00Z"))));
    assert_eq!(one.created_at, at("2024-02-10T00:00:00Z"));
    assert!(releases.find_by_tag(repo.repo.repo.id, "v2.0").await.unwrap().unwrap().is_draft);
    let mut download = fx.services.releases.open_asset(&repo, "v1.0", "rocket.tar.gz").await.unwrap();
    let mut bytes = Vec::new();
    download.file.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes, RELEASE_FILE);

    // The LFS object is stored and linked.
    let (oid, _) = forge.lfs_object();
    let oid = LfsOid::parse(&oid).unwrap();
    assert!(fx.database.lfs().is_linked(repo.repo.repo.id, &oid).await.unwrap());
    let object = fx.services.lfs.open(&repo, Some(&alice), &oid).await.unwrap();
    assert_eq!(object.size, u64::try_from(LFS_CONTENT.len()).unwrap());

    // The token went to the API as a bearer token and to Git and LFS as a password,
    // never to the LFS storage, and nothing but the repository's creation was published.
    let requests = forge.requests();
    assert!(
        requests
            .iter()
            .filter(|(what, _)| what.starts_with("GET /repos") || what.starts_with("GET /issues"))
            .all(|(_, auth)| auth.as_deref() == Some("Bearer ghp_secret123"))
    );
    assert!(requests.iter().any(|(what, auth)| what.contains("info/refs") && auth.as_deref().is_some_and(|a| a.starts_with("Basic "))));
    assert!(
        requests
            .iter()
            .filter(|(what, _)| what.starts_with("GET /lfs/"))
            .all(|(_, auth)| auth.as_deref() == Some("RemoteAuth lfs-download"))
    );
    assert!(requests.iter().filter(|(what, _)| what == "GET /issues/comments").count() >= 2, "the rate limit was waited out");
    let mut published = Vec::new();
    while let Ok(event) = events.try_recv() {
        published.push(event);
    }
    assert!(matches!(published.as_slice(), [DomainEvent::RepositoryCreated { .. }]), "{published:?}");
}

#[tokio::test]
async fn gitea_imports_can_map_authors_to_local_accounts() {
    let forge = FakeForge::start(TOKEN, None).await;
    let fx = Fixture::new(|config| config.imports.author_mapping = AuthorMapping::EmailOrUsername).await;
    let alice = fx.user("alice").await;
    let octocat = fx.user("octocat").await;
    fx.database.recovery().mark_verified(octocat.id(), &"octocat@example.com".parse().unwrap(), Timestamp(1)).await.unwrap();
    let options = ImportOptions { releases: false, lfs: false, ..ImportOptions::default() };
    let created = fx.services.imports.start(&alice, ImportInput { options, ..input(&forge, SourceKind::Gitea, "copy") }).await.unwrap();
    assert_eq!(created.repo.name.as_str(), "copy");
    let repo = fx.open(&alice, "copy").await;
    let import = fx.finished(&alice, &repo).await;
    assert_eq!(import.status, ImportStatus::Succeeded, "{:?}", import.error);
    assert_eq!((import.counts.releases, import.counts.lfs_objects), (0, 0), "skipped as asked");

    let crash = fx.thread(&repo, 3).await;
    assert_eq!(crash.author, Some(octocat.id()), "mapped by user name");
    assert_eq!(crash.body.as_str(), "Body of #3", "mapped authors need no attribution");
    let assignees = fx.database.issue_meta().assignees_of(&[fx.thread(&repo, 6).await.id]).await.unwrap();
    assert_eq!(assignees.iter().map(|(_, a)| a.id).collect::<Vec<_>>(), [octocat.id()]);
    let comments = fx.comments(&crash).await;
    assert_eq!(comments[0].author, Some(alice.id()), "hubot has no local account");
    assert_eq!(comments[1].author, Some(octocat.id()));
    assert!(forge.requests().iter().any(|(what, auth)| what.starts_with("GET /repos") && auth.as_deref() == Some("token ghp_secret123")));
}

#[tokio::test]
async fn sources_are_checked_before_anything_is_created() {
    let forge = FakeForge::start(TOKEN, None).await;
    let fx = Fixture::new(|_| {}).await;
    let alice = fx.user("alice").await;

    let wrong_token = ImportInput { token: "ghp_wrong".to_owned(), ..input(&forge, SourceKind::GitHubEnterprise, "") };
    let result = fx.services.imports.start(&alice, wrong_token).await;
    assert!(matches!(&result, Err(ServiceError::Invalid(message)) if message.contains("token")), "{result:?}");
    let unknown = ImportInput { repository: "acme/missing".to_owned(), ..input(&forge, SourceKind::GitHubEnterprise, "") };
    assert!(matches!(fx.services.imports.start(&alice, unknown).await, Err(ServiceError::Invalid(_))));
    let bad_token = ImportInput { token: "two words".to_owned(), ..input(&forge, SourceKind::GitHubEnterprise, "") };
    assert!(matches!(fx.services.imports.start(&alice, bad_token).await, Err(ServiceError::Invalid(_))));
    assert!(fx.services.repositories.open(Some(&alice), "alice", "rocket").await.is_err(), "nothing was created");

    let strict = Fixture::new(|config| config.imports.allow_private_addresses = false).await;
    let bob = strict.user("bob").await;
    let result = strict.services.imports.start(&bob, input(&forge, SourceKind::GitHubEnterprise, "")).await;
    assert!(matches!(&result, Err(ServiceError::Invalid(message)) if message.contains("public")), "{result:?}");
    let disabled = Fixture::new(|config| config.imports.enabled = false).await;
    let carol = disabled.user("carol").await;
    assert!(matches!(
        disabled.services.imports.start(&carol, input(&forge, SourceKind::GitHubEnterprise, "")).await,
        Err(ServiceError::Invalid(_))
    ));
}

#[tokio::test]
async fn a_failed_copy_leaves_a_usable_repository_and_says_why() {
    // The API works, but its clone URL points at a port nobody listens on.
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead = format!("http://{}/acme/rocket.git", closed.local_addr().unwrap());
    drop(closed);
    let forge = FakeForge::start(TOKEN, Some(&dead)).await;
    let fx = Fixture::new(|_| {}).await;
    let alice = fx.user("alice").await;
    let bob = fx.user("bob").await;
    fx.services.imports.start(&alice, input(&forge, SourceKind::GitHubEnterprise, "")).await.unwrap();
    let repo = fx.open(&alice, "rocket").await;
    let import = fx.finished(&alice, &repo).await;
    assert_eq!((import.status, import.phase), (ImportStatus::Failed, ImportPhase::Repository));
    assert!(import.error.as_deref().is_some_and(|e| e.starts_with("copying the Git repository failed")), "{:?}", import.error);
    assert!(!import.error.unwrap().contains(TOKEN));
    assert!(repo.reader.refs().await.unwrap().is_empty(), "an empty, ordinary repository remains");

    // Only the importer and the repository's administrators see the import.
    let input = rbit_services::CreateRepository { name: "public".into(), visibility: Visibility::Public, ..Default::default() };
    fx.services.repositories.create(&alice, input).await.unwrap();
    let public = fx.open(&alice, "public").await;
    assert!(matches!(fx.services.imports.status(&alice, &public).await, Err(ServiceError::NotFound)), "not imported");
    fx.database.repositories().grant_collaborator(repo.repo.repo.id, bob.id(), rbit_core::Permission::Write).await.unwrap();
    let as_bob = fx.services.repositories.open(Some(&bob), "alice", "rocket").await.unwrap();
    assert!(matches!(fx.services.imports.status(&bob, &as_bob).await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn imports_a_restart_interrupted_are_marked_failed() {
    let fx = Fixture::new(|_| {}).await;
    let alice = fx.user("alice").await;
    let input = rbit_services::CreateRepository { name: "rocket".into(), visibility: Visibility::Public, ..Default::default() };
    let created = fx.services.repositories.create(&alice, input).await.unwrap();
    let record = rbit_storage::sqlite::imports::NewImport {
        repo: created.repo.id,
        importer: alice.id(),
        kind: SourceKind::GitHub,
        source_url: "https://github.com/acme/rocket",
        options: ImportOptions::default(),
    };
    fx.database.imports().create(&record).await.unwrap();
    fx.services.spawn_background();
    let repo = fx.open(&alice, "rocket").await;
    let import = fx.finished(&alice, &repo).await;
    assert_eq!(import.status, ImportStatus::Failed);
    assert_eq!(import.error.as_deref(), Some(rbit_services::imports::INTERRUPTED));
    fx.services.shutdown().await;
}
