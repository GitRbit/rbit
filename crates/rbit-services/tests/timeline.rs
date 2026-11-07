//! The shared timeline service (T0) against real stores: who may do what, which domain
//! events are published, locked threads, throttling and what the timeline shows whom.

use std::sync::Arc;

use rbit_core::{
    Config, IssueKind, IssueNumber, IssueText, IssueTitle, RepoAccess, TimelineEvent, Timestamp, User, Visibility, issue::IssueAction,
    membership::Permission,
};
use rbit_services::{CreateRepository, DomainEvent, IssueContext, RepoContext, ServiceError, Services, timeline::TimelineEntry};
use rbit_storage::{Database, KvStore, NewUser, sqlite::issues::NewIssue};
use tokio::sync::broadcast::{Receiver, error::TryRecvError};

struct Fixture {
    directory: tempfile::TempDir,
    database: Database,
    services: Services,
    alice: User,
    bob: User,
}

impl Fixture {
    async fn new(configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        configure(&mut config);
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let alice = user(&database, "alice").await;
        let bob = user(&database, "bob").await;
        let input = CreateRepository { name: "demo".to_owned(), description: String::new(), visibility: Visibility::Public };
        services.repositories.create(&alice, input).await.unwrap();
        Self { directory: dir, database, services, alice, bob }
    }

    async fn repo(&self, viewer: Option<&User>) -> RepoContext {
        self.services.repositories.open(viewer, "alice", "demo").await.unwrap()
    }

    /// A thread in `alice/demo` opened by `author`, seeded through the store.
    async fn seed(&self, author: &User, kind: IssueKind, title: &str) -> IssueNumber {
        let repo = self.repo(Some(author)).await;
        let new = NewIssue {
            repo: repo.repo.repo.id,
            kind,
            title: IssueTitle::parse(title).unwrap(),
            body: IssueText::body("Steps to reproduce").unwrap(),
            author: author.id(),
            at: Timestamp::now(),
        };
        self.database.issues().create(&new).await.unwrap().number
    }

    async fn open(&self, viewer: &User, number: IssueNumber) -> IssueContext {
        let repo = self.repo(Some(viewer)).await;
        self.services.timeline.open(Some(viewer), repo, number).await.unwrap()
    }
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

/// Issue and comment events published so far (others are ignored).
fn drain(events: &mut Receiver<DomainEvent>) -> Vec<DomainEvent> {
    let mut seen = Vec::new();
    loop {
        match events.try_recv() {
            Ok(event @ (DomainEvent::Issue { .. } | DomainEvent::IssueComment { .. })) => seen.push(event),
            Ok(_) => {}
            Err(TryRecvError::Empty | TryRecvError::Closed) => return seen,
            Err(TryRecvError::Lagged(_)) => panic!("lagged"),
        }
    }
}

fn issue_actions(events: &[DomainEvent]) -> Vec<IssueAction> {
    events.iter().filter_map(|event| if let DomainEvent::Issue { action, .. } = event { Some(action.clone()) } else { None }).collect()
}

#[tokio::test]
async fn opening_a_thread_resolves_its_author_and_permissions() {
    let fixture = Fixture::new(|_| {}).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Crash").await;

    let owner = fixture.open(&fixture.alice, number).await;
    assert_eq!(owner.author.as_ref().map(|a| a.name.as_str()), Some("alice"));
    assert_eq!(owner.repo.access, RepoAccess::Admin);
    assert!(owner.permissions.can_delete && owner.permissions.can_lock && owner.permissions.can_edit);

    let reader = fixture.open(&fixture.bob, number).await;
    assert!(reader.permissions.can_comment);
    assert!(!reader.permissions.can_edit && !reader.permissions.can_set_state && !reader.permissions.can_lock);

    let anonymous = fixture.services.timeline.open(None, fixture.repo(None).await, number).await.unwrap();
    assert!(!anonymous.permissions.can_comment, "reading needs no account, commenting does");

    let missing = IssueNumber::new(99).unwrap();
    let result = fixture.services.timeline.open(Some(&fixture.bob), fixture.repo(Some(&fixture.bob)).await, missing).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));

    let counts = fixture.services.timeline.open_counts(owner.repo.repo.repo.id).await.unwrap();
    assert_eq!((counts.issues, counts.pulls), (1, 0));
}

mod human_comments_port;
mod lifecycle_port;
mod reader_port;
mod task_comments_port;

mod text_edit_port;

mod state_change_port;

#[tokio::test]
async fn locked_threads_admit_only_writers() {
    let fixture = Fixture::new(|_| {}).await;
    let timeline = &fixture.services.timeline;
    let owner_repo = fixture.repo(Some(&fixture.alice)).await;
    let dave = user(&fixture.database, "dave").await;
    fixture.services.collaborators.add(&fixture.alice, &owner_repo, "dave", Permission::Write).await.unwrap();
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Heated").await;
    let as_bob = fixture.open(&fixture.bob, number).await;
    let early = timeline.comment(&fixture.bob, &as_bob, "First!").await.unwrap();
    let mut events = fixture.services.events.subscribe();

    assert!(matches!(timeline.set_locked(&fixture.bob, &as_bob, true).await, Err(ServiceError::Forbidden)));
    let as_dave = fixture.open(&dave, number).await;
    timeline.set_locked(&dave, &as_dave, true).await.unwrap();
    timeline.set_locked(&dave, &as_dave, true).await.unwrap();

    let locked_bob = fixture.open(&fixture.bob, number).await;
    assert!(!locked_bob.permissions.can_comment);
    assert!(matches!(timeline.comment(&fixture.bob, &locked_bob, "Hello?").await, Err(ServiceError::Forbidden)));
    assert!(matches!(timeline.edit_comment(&fixture.bob, &locked_bob, early.id, "Edited").await, Err(ServiceError::Forbidden)));
    let locked_dave = fixture.open(&dave, number).await;
    timeline.comment(&dave, &locked_dave, "Cooling off.").await.unwrap();
    timeline.delete_comment(&dave, &locked_dave, early.id).await.unwrap();

    let entries = timeline.timeline(Some(&fixture.bob), &locked_bob, None, 50).await.unwrap().entries;
    assert!(
        entries
            .iter()
            .all(|entry| !matches!(entry, TimelineEntry::Comment { can_edit: true, .. } | TimelineEntry::Comment { can_delete: true, .. }))
    );
    let entries = timeline.timeline(Some(&dave), &locked_dave, None, 50).await.unwrap().entries;
    assert!(entries.iter().any(|entry| matches!(entry, TimelineEntry::Comment { can_edit: true, can_delete: true, .. })));

    timeline.set_locked(&dave, &locked_dave, false).await.unwrap();
    assert!(fixture.open(&fixture.bob, number).await.permissions.can_comment);
    assert_eq!(issue_actions(&drain(&mut events)), [IssueAction::Locked, IssueAction::Unlocked], "no event for a no-op");
}

#[tokio::test]
async fn content_is_throttled_per_account() {
    let fixture = Fixture::new(|_| {}).await;
    let timeline = &fixture.services.timeline;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Spam target").await;
    let as_bob = fixture.open(&fixture.bob, number).await;
    for i in 0..30 {
        timeline.comment(&fixture.bob, &as_bob, &format!("comment {i}")).await.unwrap();
    }
    let throttled = timeline.comment(&fixture.bob, &as_bob, "one more").await;
    assert!(matches!(throttled, Err(ServiceError::RateLimited { retry_after_seconds }) if retry_after_seconds > 0));
    let as_alice = fixture.open(&fixture.alice, number).await;
    timeline.comment(&fixture.alice, &as_alice, "others are unaffected").await.unwrap();
    assert!(matches!(timeline.edit(&fixture.bob, &as_bob, None, Some("x")).await, Err(ServiceError::Forbidden)), "permission first");
}

#[tokio::test]
async fn throttling_can_be_disabled() {
    let fixture = Fixture::new(|config| config.security.rate_limit = false).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Busy").await;
    let as_bob = fixture.open(&fixture.bob, number).await;
    for i in 0..40 {
        fixture.services.timeline.comment(&fixture.bob, &as_bob, &format!("comment {i}")).await.unwrap();
    }
}

#[tokio::test]
async fn the_timeline_names_subjects_and_hides_unreadable_cross_references() {
    let fixture = Fixture::new(|_| {}).await;
    let timeline = &fixture.services.timeline;
    let carol = user(&fixture.database, "carol").await;
    let input = CreateRepository { name: "secret".to_owned(), description: String::new(), visibility: Visibility::Private };
    fixture.services.repositories.create(&carol, input).await.unwrap();
    let secret = fixture.services.repositories.open(Some(&carol), "carol", "secret").await.unwrap();
    let hidden = fixture
        .database
        .issues()
        .create(&NewIssue {
            repo: secret.repo.repo.id,
            kind: IssueKind::Pull,
            title: IssueTitle::parse("Secret plan").unwrap(),
            body: IssueText::default(),
            author: carol.id(),
            at: Timestamp::now(),
        })
        .await
        .unwrap();

    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Public").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let store = fixture.database.issues();
    let at = Timestamp(Timestamp::now().0 - 60);
    store.record_event(ctx.issue.id, Some(fixture.alice.id()), &TimelineEvent::Assigned { assignee: fixture.bob.id() }, at).await.unwrap();
    store.record_event(ctx.issue.id, Some(carol.id()), &TimelineEvent::CrossReferenced { source: hidden.id }, at).await.unwrap();
    timeline.comment(&fixture.bob, &fixture.open(&fixture.bob, number).await, "On it").await.unwrap();

    let as_bob = fixture.open(&fixture.bob, number).await;
    let for_bob = timeline.timeline(Some(&fixture.bob), &as_bob, None, 50).await.unwrap();
    assert_eq!(for_bob.entries.len(), 2, "the cross-reference from a private repository is dropped");
    let TimelineEntry::Event { subjects, .. } = &for_bob.entries[0] else { panic!("the assignment") };
    assert_eq!(subjects.account.as_ref().map(|a| a.name.as_str()), Some("bob"));
    assert!(matches!(&for_bob.entries[1], TimelineEntry::Comment { can_edit: true, can_delete: true, .. }));
    let anonymous = fixture.services.timeline.open(None, fixture.repo(None).await, number).await.unwrap();
    assert_eq!(timeline.timeline(None, &anonymous, None, 50).await.unwrap().entries.len(), 2);

    let as_carol = fixture.open(&carol, number).await;
    let for_carol = timeline.timeline(Some(&carol), &as_carol, None, 50).await.unwrap();
    assert_eq!(for_carol.entries.len(), 3);
    let TimelineEntry::Event { subjects, event: TimelineEvent::CrossReferenced { .. }, .. } = &for_carol.entries[1] else {
        panic!("the cross-reference")
    };
    let source = subjects.source.as_ref().unwrap();
    assert_eq!(
        (source.full_name.as_str(), source.number.get(), source.kind, source.title.as_str()),
        ("carol/secret", 1, IssueKind::Pull, "Secret plan")
    );
    assert!(matches!(&for_carol.entries[2], TimelineEntry::Comment { can_edit: false, can_delete: false, .. }));

    let first = timeline.timeline(Some(&carol), &as_carol, None, 0).await.unwrap();
    assert_eq!(first.entries.len(), 1, "the limit is at least one");
    let rest = timeline.timeline(Some(&carol), &as_carol, first.next, 500).await.unwrap();
    assert_eq!(rest.entries.len(), 2);
    assert!(rest.next.is_none());
}
