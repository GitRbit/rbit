//! Email notifications (workstream N) against real stores and a memory mail transport:
//! who receives mail, that each notification is mailed once, preferences, visibility,
//! threading headers and the one-click unsubscribe link.

use std::{sync::Arc, time::Duration};

use rbit_core::{
    Config, IssueKind, IssueNumber, StateReason, Timestamp, User, Visibility,
    config::EmailPreference,
    issue::{IssueAction, ReviewAction, ReviewState},
    membership::Permission,
};
use rbit_mail::{DisabledMailer, Mail, MemoryMailer};
use rbit_services::{
    CreateRepository, DomainEvent, IssueContext, MailService, NotificationService, RepoContext, ServiceError, Services,
    issues::NewIssueInput, timeline::StateChange,
};
use rbit_storage::{
    Database, KvStore, NewUser,
    sqlite::{
        issues::NewIssue,
        pulls::{CreatedPull, NewPull},
        reviews::NewReview,
    },
};
use tokio::sync::broadcast::{Receiver, error::TryRecvError};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
    /// The notification service, mailing into `outbox`.
    notifications: NotificationService,
    outbox: MemoryMailer,
    alice: User,
    bob: User,
    carol: User,
}

async fn user(database: &Database, name: &str, verified: bool) -> User {
    let email: rbit_core::Email = format!("{name}@example.com").parse().unwrap();
    let user = database
        .accounts()
        .create_user(NewUser {
            name: name.parse().unwrap(),
            email: email.clone(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        })
        .await
        .unwrap();
    if verified {
        assert!(database.recovery().mark_verified(user.id(), &email, Timestamp::now()).await.unwrap());
    }
    user
}

impl Fixture {
    /// alice owns `alice/demo`; alice and bob have verified addresses, carol does not.
    async fn new(visibility: Visibility, configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        "https://git.example.com".clone_into(&mut config.server.public_url);
        configure(&mut config);
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let outbox = MemoryMailer::new();
        let notifications = services.notifications.with_email(services.notification_email.with_mail(mail_service(&outbox)));
        let alice = user(&database, "alice", true).await;
        let bob = user(&database, "bob", true).await;
        let carol = user(&database, "carol", false).await;
        let input = CreateRepository { name: "demo".to_owned(), description: String::new(), visibility };
        services.repositories.create(&alice, input).await.unwrap();
        Self { _dir: dir, database, services, notifications, outbox, alice, bob, carol }
    }

    async fn repo(&self, viewer: &User) -> RepoContext {
        self.services.repositories.open(Some(viewer), "alice", "demo").await.unwrap()
    }

    async fn open(&self, viewer: &User, number: IssueNumber) -> IssueContext {
        self.services.timeline.open(Some(viewer), self.repo(viewer).await, number).await.unwrap()
    }

    async fn create_issue(&self, body: &str) -> rbit_core::Issue {
        let input = NewIssueInput { title: "Crash".into(), body: body.into(), ..NewIssueInput::default() };
        self.services.issues.create(&self.alice, &self.repo(&self.alice).await, input).await.unwrap()
    }

    /// Delivers every event published so far, as the background subscriber would, and
    /// lets the mail queue run.
    async fn deliver(&self, events: &mut Receiver<DomainEvent>) {
        loop {
            match events.try_recv() {
                Ok(event) => self.notifications.handle_event(&event).await.unwrap(),
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
                Err(TryRecvError::Lagged(_)) => panic!("lagged"),
            }
        }
        settle().await;
    }

    /// Every message sent so far to `name@example.com`.
    fn mail_to(&self, name: &str) -> Vec<Mail> {
        let address = format!("{name}@example.com");
        self.outbox.sent().into_iter().filter(|mail| mail.to.as_str() == address).collect()
    }
}

fn mail_service(outbox: &MemoryMailer) -> MailService {
    MailService::with_mailer(Arc::new(outbox.clone()), Duration::from_secs(5))
}

/// Lets queued deliveries (spawned tasks) finish: the memory transport completes at once.
async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn mentions_reach_verified_addresses_and_never_the_actor() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let mut events = fixture.services.events.subscribe();

    let issue = fixture.create_issue("Broken since today, cc @bob and @carol").await;
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.outbox.sent().len(), 1, "only bob: carol is unverified and alice acted");
    let mail = &fixture.mail_to("bob")[0];
    assert_eq!(mail.subject, "[alice/demo] Crash (#1)");
    assert!(mail.text.starts_with("alice opened issue #1.\n\nBroken since today, cc @bob and @carol"), "{}", mail.text);
    assert!(mail.text.contains("https://git.example.com/alice/demo/issues/1\n"), "links to the thread");
    assert!(mail.text.contains("You are receiving this because you were mentioned."));
    assert!(mail.text.contains("https://git.example.com/settings/notifications"));
    assert_eq!(mail.headers.message_id.as_deref(), Some("<alice/demo/issues/1@git.example.com>"), "the thread's own id");
    assert_eq!(mail.headers.in_reply_to, None);
    let unsubscribe = mail.headers.one_click_unsubscribe.clone().unwrap();
    assert!(unsubscribe.starts_with(&format!("https://git.example.com/email/unsubscribe/{}.", fixture.bob.id().0)));
    assert!(mail.text.contains(&unsubscribe), "the footer has the same link");
    // The inbox has carol's notification all the same.
    assert_eq!(fixture.services.notifications.inbox(&fixture.carol, true, 1).await.unwrap().total, 1);

    // Bob comments: alice (the author) hears about it in the same thread; bob does not.
    let as_bob = fixture.open(&fixture.bob, issue.number).await;
    let comment = fixture.services.timeline.comment(&fixture.bob, &as_bob, "On it").await.unwrap();
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.mail_to("bob").len(), 1, "the actor gets nothing");
    let reply = &fixture.mail_to("alice")[0];
    assert_eq!(reply.subject, "Re: [alice/demo] Crash (#1)");
    assert!(reply.text.starts_with("bob commented on issue #1.\n\nOn it"));
    assert!(reply.text.contains(&format!("https://git.example.com/alice/demo/issues/1#issuecomment-{}", comment.id.0)));
    assert!(reply.text.contains("because you opened this thread"));
    assert_eq!(reply.headers.in_reply_to.as_deref(), Some("<alice/demo/issues/1@git.example.com>"));
    assert_eq!(
        reply.headers.message_id,
        Some(format!("<alice/demo/issues/1/comment-{}@git.example.com>", comment.id.0)),
        "each message has its own id"
    );

    // Closing reaches bob (a commenter now) with the state change.
    let as_alice = fixture.open(&fixture.alice, issue.number).await;
    fixture.services.timeline.set_state(&fixture.alice, &as_alice, StateChange::Close(StateReason::Completed)).await.unwrap();
    fixture.deliver(&mut events).await;
    let closed = fixture.mail_to("bob").pop().unwrap();
    assert!(closed.text.starts_with("alice closed issue #1 as completed."), "{}", closed.text);
    assert!(closed.text.contains("because you were mentioned"), "the strongest reason is given");
}

#[tokio::test]
async fn each_notification_is_mailed_once() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let mut events = fixture.services.events.subscribe();
    fixture.create_issue("@bob please look").await;
    let opened: Vec<DomainEvent> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    for event in opened.iter().chain(&opened) {
        fixture.notifications.handle_event(event).await.unwrap();
    }
    settle().await;
    assert_eq!(fixture.mail_to("bob").len(), 1, "handling the same event twice mails once");
}

#[tokio::test]
async fn preferences_decide_what_is_mailed() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let mut events = fixture.services.events.subscribe();
    let email = &fixture.services.notification_email;
    let issue = fixture.create_issue("No mentions").await;
    fixture.deliver(&mut events).await;

    // Bob subscribes by hand: "participating" (the default) keeps that thread in the inbox only.
    let settings = email.settings(&fixture.bob).await.unwrap();
    assert_eq!(settings.preference, EmailPreference::Participating);
    assert!(!settings.chosen && settings.verified);
    assert!(!settings.available, "the configured transport is `disabled`; the fixture mails through its own");
    fixture.services.notifications.set_subscribed(fixture.bob.id(), issue.id, true).await.unwrap();
    let as_alice = fixture.open(&fixture.alice, issue.number).await;
    fixture.services.timeline.comment(&fixture.alice, &as_alice, "First update").await.unwrap();
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.mail_to("bob"), []);
    assert_eq!(fixture.services.notifications.inbox(&fixture.bob, true, 1).await.unwrap().total, 1, "but the inbox has it");

    // "All" includes it.
    email.set_preference(&fixture.bob, EmailPreference::All).await.unwrap();
    fixture.services.timeline.comment(&fixture.alice, &as_alice, "Second update").await.unwrap();
    fixture.deliver(&mut events).await;
    let mails = fixture.mail_to("bob");
    assert_eq!(mails.len(), 1);
    assert!(mails[0].text.contains("because you are subscribed to this thread"));

    // "Off" stops everything, mentions included.
    email.set_preference(&fixture.bob, EmailPreference::Off).await.unwrap();
    fixture.services.timeline.comment(&fixture.alice, &as_alice, "Third update, @bob").await.unwrap();
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.mail_to("bob").len(), 1);
    let settings = email.settings(&fixture.bob).await.unwrap();
    assert_eq!((settings.preference, settings.chosen), (EmailPreference::Off, true));

    // Unverified addresses get nothing whatever they chose.
    email.set_preference(&fixture.carol, EmailPreference::All).await.unwrap();
    fixture.services.timeline.comment(&fixture.alice, &as_alice, "Hello @carol").await.unwrap();
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.mail_to("carol"), []);
    assert!(!email.settings(&fixture.carol).await.unwrap().verified);
}

#[tokio::test]
async fn nobody_is_mailed_about_a_repository_they_cannot_read() {
    let fixture = Fixture::new(Visibility::Private, |_| {}).await;
    let mut events = fixture.services.events.subscribe();
    let repo = fixture.repo(&fixture.alice).await;
    fixture.services.collaborators.add(&fixture.alice, &repo, "bob", Permission::Read).await.unwrap();
    // A verified outsider, mentioned in the private repository.
    let _dave = user(&fixture.database, "dave", true).await;

    let issue = fixture.create_issue("@bob @dave").await;
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.mail_to("bob").len(), 1);
    assert!(fixture.mail_to("dave").is_empty(), "an outsider hears nothing");

    // Bob loses access: the next comment does not reach him.
    fixture.services.collaborators.remove(&fixture.alice, &repo, "bob").await.unwrap();
    let as_alice = fixture.open(&fixture.alice, issue.number).await;
    fixture.services.timeline.comment(&fixture.alice, &as_alice, "Still secret").await.unwrap();
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.mail_to("bob").len(), 1, "no mail after losing access");

    // Suspended users get nothing either.
    fixture.services.collaborators.add(&fixture.alice, &repo, "bob", Permission::Read).await.unwrap();
    fixture.database.admin().set_suspended(fixture.bob.id(), true).await.unwrap();
    fixture.services.timeline.comment(&fixture.alice, &as_alice, "Anyone?").await.unwrap();
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.mail_to("bob").len(), 1);
}

#[tokio::test]
async fn nothing_is_sent_without_a_transport_or_with_email_off() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let disabled = fixture.services.notifications.with_email(
        fixture.services.notification_email.with_mail(MailService::with_mailer(Arc::new(DisabledMailer), Duration::from_secs(1))),
    );
    let mut events = fixture.services.events.subscribe();
    let issue = fixture.create_issue("@bob").await;
    let as_alice = fixture.open(&fixture.alice, issue.number).await;
    fixture.services.timeline.comment(&fixture.alice, &as_alice, "Ping").await.unwrap();
    let published: Vec<DomainEvent> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    for event in &published {
        disabled.handle_event(event).await.unwrap();
    }
    settle().await;
    assert!(fixture.services.notifications.has_unread(fixture.bob.id()).await.unwrap(), "the inbox still works");
    assert!(!fixture.services.notification_email.available(), "the default configuration has no transport");

    // Nothing was claimed while email was unavailable: the comment can still be mailed.
    let comment = published.iter().find(|event| matches!(event, DomainEvent::IssueComment { .. })).unwrap();
    fixture.notifications.handle_event(comment).await.unwrap();
    settle().await;
    assert_eq!(fixture.mail_to("bob").len(), 1);

    let off = Fixture::new(Visibility::Public, |config| config.notifications.email = false).await;
    assert!(!off.services.notification_email.with_mail(mail_service(&off.outbox)).available());
    let mut events = off.services.events.subscribe();
    off.create_issue("@bob").await;
    off.deliver(&mut events).await;
    assert_eq!(off.outbox.sent(), []);
    assert!(off.services.notifications.has_unread(off.bob.id()).await.unwrap(), "the inbox still works");

    let quiet = Fixture::new(Visibility::Public, |config| config.notifications.default_email = EmailPreference::Off).await;
    let mut events = quiet.services.events.subscribe();
    quiet.create_issue("@bob").await;
    quiet.deliver(&mut events).await;
    assert!(quiet.outbox.sent().is_empty(), "the configured default applies to users who never chose");
}

#[tokio::test]
async fn unsubscribe_links_turn_email_off() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let email = &fixture.services.notification_email;
    let link = email.unsubscribe_link(fixture.bob.id()).await.unwrap();
    assert_eq!(link, email.unsubscribe_link(fixture.bob.id()).await.unwrap(), "links are stable");
    let token = link.rsplit('/').next().unwrap().to_owned();

    assert_eq!(email.unsubscribe_target(&token).await.unwrap(), "bob");
    let forged = token.replacen(&format!("{}.", fixture.bob.id().0), &format!("{}.", fixture.alice.id().0), 1);
    assert!(matches!(email.unsubscribe_target(&forged).await, Err(ServiceError::NotFound)), "a token names one account only");
    assert!(matches!(email.unsubscribe(&forged).await, Err(ServiceError::NotFound)));
    assert!(matches!(email.unsubscribe("garbage").await, Err(ServiceError::NotFound)));
    assert_eq!(email.settings(&fixture.alice).await.unwrap().preference, EmailPreference::Participating, "untouched");

    assert_eq!(email.unsubscribe(&token).await.unwrap(), "bob");
    assert_eq!(email.unsubscribe(&token).await.unwrap(), "bob", "repeating it is harmless");
    assert_eq!(email.settings(&fixture.bob).await.unwrap().preference, EmailPreference::Off);

    let mut events = fixture.services.events.subscribe();
    fixture.create_issue("@bob").await;
    fixture.deliver(&mut events).await;
    assert!(fixture.outbox.sent().is_empty(), "no more mail");
}

#[tokio::test]
async fn reviews_requests_and_merges_are_mailed() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let repo = fixture.repo(&fixture.alice).await;
    let new = NewPull {
        issue: NewIssue {
            repo: repo.repo.repo.id,
            kind: IssueKind::Pull,
            title: rbit_core::IssueTitle::parse("Fix").unwrap(),
            body: rbit_core::IssueText::default(),
            author: fixture.alice.id(),
            at: Timestamp::now(),
        },
        head_repo: repo.repo.repo.id,
        head_branch: "fix".to_owned(),
        base_branch: "main".to_owned(),
        head_sha: "0123456789012345678901234567890123456789".to_owned(),
        is_draft: false,
    };
    let CreatedPull::Created(created) = fixture.database.pulls().create(&new).await.unwrap() else { panic!("not a duplicate") };
    let pull = created.0;
    let issue_event = |actor: &User, action| DomainEvent::Issue {
        repo: repo.repo.repo.id,
        issue: pull.id,
        number: pull.number,
        kind: IssueKind::Pull,
        actor: Some(actor.id()),
        action,
    };
    let notifications = &fixture.notifications;
    notifications.handle_event(&issue_event(&fixture.alice, IssueAction::Opened)).await.unwrap();
    notifications.handle_event(&issue_event(&fixture.alice, IssueAction::ReviewRequested { reviewer: fixture.bob.id() })).await.unwrap();
    settle().await;
    let requested = fixture.mail_to("bob").pop().unwrap();
    assert!(requested.text.starts_with("alice requested your review on pull request #1."), "{}", requested.text);
    assert!(requested.text.contains("https://git.example.com/alice/demo/pulls/1\n"));
    assert!(requested.text.contains("because your review was requested"));
    assert!(requested.html.contains("View pull request"));
    assert_eq!(requested.headers.in_reply_to.as_deref(), Some("<alice/demo/pulls/1@git.example.com>"));

    let (review, _) = fixture
        .database
        .reviews()
        .create_submitted(
            &NewReview {
                pull: pull.id,
                reviewer: fixture.bob.id(),
                state: ReviewState::Approved,
                body: "Looks good",
                commit_sha: "0123456789012345678901234567890123456789",
                at: Timestamp::now(),
            },
            &[],
        )
        .await
        .unwrap();
    let submitted = DomainEvent::PullRequestReview {
        repo: repo.repo.repo.id,
        pull: pull.id,
        number: pull.number,
        review: review.id,
        actor: fixture.bob.id(),
        action: ReviewAction::Submitted,
    };
    notifications.handle_event(&submitted).await.unwrap();
    settle().await;
    let approved = fixture.mail_to("alice").pop().unwrap();
    assert!(approved.text.starts_with("bob approved pull request #1.\n\nLooks good"), "{}", approved.text);
    assert!(approved.text.contains(&format!("#pullrequestreview-{}", review.id.0)));

    notifications.handle_event(&issue_event(&fixture.alice, IssueAction::Closed { reason: StateReason::Merged })).await.unwrap();
    settle().await;
    let merged = fixture.mail_to("bob").pop().unwrap();
    assert!(merged.text.starts_with("alice merged pull request #1."), "{}", merged.text);
    assert!(merged.text.contains("because your review was requested"), "the strongest reason is given");
}
