//! The shared timeline service (T0) against real stores: who may do what, which domain
//! events are published, locked threads, throttling and what the timeline shows whom.

use std::sync::Arc;

use rbit_core::{
    Config, IssueKind, IssueNumber, IssueState, IssueText, IssueTitle, RepoAccess, StateReason, TimelineEvent, Timestamp, User, Visibility,
    issue::{CommentAction, IssueAction},
    membership::Permission,
};
use rbit_services::{
    CreateRepository, DomainEvent, IssueContext, RepoContext, ServiceError, Services,
    timeline::{StateChange, TimelineEntry},
};
use rbit_storage::{Database, KvStore, NewUser, sqlite::issues::NewIssue};
use tokio::sync::broadcast::{Receiver, error::TryRecvError};

struct Fixture {
    _dir: tempfile::TempDir,
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
        Self { _dir: dir, database, services, alice, bob }
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

#[tokio::test]
async fn comments_follow_the_permission_table_and_publish_events() {
    let fixture = Fixture::new(|_| {}).await;
    let timeline = &fixture.services.timeline;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Crash").await;
    let mut events = fixture.services.events.subscribe();

    let as_bob = fixture.open(&fixture.bob, number).await;
    let as_alice = fixture.open(&fixture.alice, number).await;
    assert!(matches!(timeline.comment(&fixture.bob, &as_bob, " \n ").await, Err(ServiceError::Invalid(_))));
    let comment = timeline.comment(&fixture.bob, &as_bob, "Same here\r\n").await.unwrap();
    assert_eq!(comment.body.as_str(), "Same here\n");

    assert!(matches!(timeline.edit_comment(&fixture.alice, &as_alice, comment.id, "Hacked").await, Err(ServiceError::Forbidden)));
    let edited = timeline.edit_comment(&fixture.bob, &as_bob, comment.id, "Same here, on Linux").await.unwrap();
    assert!(edited.edited_at.is_some());
    let unchanged = timeline.edit_comment(&fixture.bob, &as_bob, comment.id, "Same here, on Linux").await.unwrap();
    assert_eq!(unchanged, edited, "the same text again is not an edit");

    let mine = timeline.comment(&fixture.alice, &as_alice, "Thanks").await.unwrap();
    assert!(matches!(timeline.delete_comment(&fixture.bob, &as_bob, mine.id).await, Err(ServiceError::Forbidden)));
    timeline.delete_comment(&fixture.alice, &as_alice, comment.id).await.unwrap();

    // A comment of another thread is not found through this one.
    let other = fixture.open(&fixture.alice, fixture.seed(&fixture.alice, IssueKind::Issue, "Other").await).await;
    assert!(matches!(timeline.delete_comment(&fixture.alice, &other, mine.id).await, Err(ServiceError::NotFound)));

    let actions: Vec<_> = drain(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            DomainEvent::IssueComment { action, actor, number: n, kind, .. } => {
                assert_eq!((n, kind), (number, IssueKind::Issue));
                Some((actor, action))
            }
            _ => None,
        })
        .collect();
    assert_eq!(actions.len(), 4);
    assert_eq!(actions[0], (Some(fixture.bob.id()), CommentAction::Created));
    assert_eq!(actions[1], (Some(fixture.bob.id()), CommentAction::Edited { previous_body: "Same here\n".to_owned() }));
    assert_eq!(actions[2], (Some(fixture.alice.id()), CommentAction::Created));
    assert!(
        matches!(&actions[3], (actor, CommentAction::Deleted { snapshot }) if *actor == Some(fixture.alice.id()) && snapshot.id == comment.id)
    );

    assert_eq!(fixture.open(&fixture.alice, number).await.issue.comment_count, 1);
}

#[tokio::test]
async fn editing_title_and_body_records_a_rename() {
    let fixture = Fixture::new(|_| {}).await;
    let timeline = &fixture.services.timeline;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Crash").await;
    let bobs = fixture.seed(&fixture.bob, IssueKind::Issue, "Bob's report").await;
    let mut events = fixture.services.events.subscribe();

    let as_bob = fixture.open(&fixture.bob, number).await;
    assert!(matches!(timeline.edit(&fixture.bob, &as_bob, Some("Mine now"), None).await, Err(ServiceError::Forbidden)));
    let own = fixture.open(&fixture.bob, bobs).await;
    timeline.edit(&fixture.bob, &own, None, Some("More detail")).await.unwrap();

    let as_alice = fixture.open(&fixture.alice, number).await;
    assert!(matches!(timeline.edit(&fixture.alice, &as_alice, Some("  "), None).await, Err(ServiceError::Invalid(_))));
    let updated = timeline.edit(&fixture.alice, &as_alice, Some("Crash on start"), Some("Steps to reproduce")).await.unwrap();
    assert_eq!(updated.title.as_str(), "Crash on start");
    let again = fixture.open(&fixture.alice, number).await;
    timeline.edit(&fixture.alice, &again, Some("Crash on start"), None).await.unwrap();

    assert_eq!(
        issue_actions(&drain(&mut events)),
        [
            IssueAction::Edited { previous_title: None, previous_body: Some("Steps to reproduce".to_owned()), previous_base: None },
            IssueAction::Edited { previous_title: Some("Crash".to_owned()), previous_body: None, previous_base: None },
        ],
        "only what changed is reported, and no change publishes nothing"
    );

    let entries = timeline.timeline(Some(&fixture.bob), &again, None, 50).await.unwrap().entries;
    assert!(matches!(
        &entries[..],
        [TimelineEntry::Event { event: TimelineEvent::Renamed { from, to }, actor: Some(actor), .. }]
            if from == "Crash" && to == "Crash on start" && actor.name.as_str() == "alice"
    ));
}

#[tokio::test]
async fn closing_and_reopening() {
    let fixture = Fixture::new(|_| {}).await;
    let timeline = &fixture.services.timeline;
    let alices = fixture.seed(&fixture.alice, IssueKind::Issue, "Crash").await;
    let bobs = fixture.seed(&fixture.bob, IssueKind::Issue, "Question").await;
    let pull = fixture.seed(&fixture.alice, IssueKind::Pull, "Fix").await;
    let mut events = fixture.services.events.subscribe();

    let not_bobs = fixture.open(&fixture.bob, alices).await;
    assert!(matches!(
        timeline.set_state(&fixture.bob, &not_bobs, StateChange::Close(StateReason::Completed)).await,
        Err(ServiceError::Forbidden)
    ));

    let own = fixture.open(&fixture.bob, bobs).await;
    let closed = timeline.set_state(&fixture.bob, &own, StateChange::Close(StateReason::NotPlanned)).await.unwrap();
    assert_eq!(
        (closed.state, closed.state_reason, closed.closed_by),
        (IssueState::Closed, Some(StateReason::NotPlanned), Some(fixture.bob.id()))
    );
    let again = timeline.set_state(&fixture.bob, &own, StateChange::Close(StateReason::Completed)).await.unwrap();
    assert_eq!(again.state_reason, Some(StateReason::NotPlanned), "already closed: unchanged");
    let reopened = timeline.set_state(&fixture.bob, &own, StateChange::Reopen).await.unwrap();
    assert_eq!(reopened.state, IssueState::Open);

    let as_alice = fixture.open(&fixture.alice, pull).await;
    let merged = timeline.set_state(&fixture.alice, &as_alice, StateChange::Close(StateReason::Merged)).await;
    assert!(matches!(merged, Err(ServiceError::Invalid(_))), "only a merge closes as merged");
    timeline.set_state(&fixture.alice, &as_alice, StateChange::Close(StateReason::Completed)).await.unwrap();
    let closed_pull = fixture.open(&fixture.alice, pull).await;
    timeline.set_state(&fixture.alice, &closed_pull, StateChange::Reopen).await.unwrap();

    let published = drain(&mut events);
    assert_eq!(
        issue_actions(&published),
        [
            IssueAction::Closed { reason: StateReason::NotPlanned },
            IssueAction::Reopened,
            IssueAction::Closed { reason: StateReason::Completed },
            IssueAction::Reopened,
        ]
    );
    assert!(matches!(published[2], DomainEvent::Issue { kind: IssueKind::Pull, .. }));

    let kinds: Vec<_> = timeline
        .timeline(None, &own, None, 50)
        .await
        .unwrap()
        .entries
        .iter()
        .map(|entry| match entry {
            TimelineEntry::Event { event, .. } => event.kind(),
            TimelineEntry::Comment { .. } => "comment",
        })
        .collect();
    assert_eq!(kinds, ["closed", "reopened"]);
}

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
