//! Email digests against real stores, a memory mail transport and a test clock: digest
//! users get no single emails, one digest per period grouped by repository, nothing for
//! threads read in the app, nothing twice (also after a restart), and only what they may
//! still read.

use std::{
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

use rbit_core::{
    Config, Timestamp, User, Visibility,
    config::{EmailDelivery, EmailPreference},
    membership::Permission,
};
use rbit_mail::{DisabledMailer, Mail, MemoryMailer};
use rbit_services::{
    CreateRepository, DomainEvent, MailService, NotificationEmailService, NotificationService, RepoContext, Services,
    issues::NewIssueInput,
    notification_email::{DigestRun, period_start},
};
use rbit_storage::{Database, KvStore, NewUser, sqlite::email_digests::Claim};
use tokio::sync::broadcast::{Receiver, error::TryRecvError};

const HOUR: i64 = 60 * 60;
const DAY: i64 = 24 * HOUR;
/// 2026-10-19 00:00 UTC, a Monday.
const MONDAY: i64 = 1_792_368_000;

struct Fixture {
    _dir: tempfile::TempDir,
    config: Arc<Config>,
    database: Database,
    kv: KvStore,
    services: Services,
    /// The email service, mailing into `outbox` and reading `clock`.
    email: NotificationEmailService,
    notifications: NotificationService,
    outbox: MemoryMailer,
    clock: Arc<AtomicI64>,
    alice: User,
    bob: User,
}

async fn user(database: &Database, name: &str) -> User {
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
    assert!(database.recovery().mark_verified(user.id(), &email, Timestamp::now()).await.unwrap());
    user
}

fn mail_service(outbox: &MemoryMailer) -> MailService {
    MailService::with_mailer(Arc::new(outbox.clone()), Duration::from_secs(5))
}

impl Fixture {
    /// alice owns `alice/demo` and `alice/tools`; both have verified addresses. The clock
    /// starts on Monday at 09:00 UTC.
    async fn new(visibility: Visibility, configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        "https://git.example.com".clone_into(&mut config.server.public_url);
        configure(&mut config);
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let outbox = MemoryMailer::new();
        let clock = Arc::new(AtomicI64::new(MONDAY + 9 * HOUR));
        let email = services.notification_email.with_mail(mail_service(&outbox)).with_clock(reader(&clock));
        let notifications = services.notifications.with_email(email.clone());
        let alice = user(&database, "alice").await;
        let bob = user(&database, "bob").await;
        for name in ["demo", "tools"] {
            let input = CreateRepository { name: name.to_owned(), description: String::new(), visibility };
            services.repositories.create(&alice, input).await.unwrap();
        }
        Self { _dir: dir, config, database, kv, services, email, notifications, outbox, clock, alice, bob }
    }

    fn set_time(&self, seconds: i64) {
        self.clock.store(seconds, Ordering::SeqCst);
    }

    async fn repo(&self, name: &str) -> RepoContext {
        self.services.repositories.open(Some(&self.alice), "alice", name).await.unwrap()
    }

    async fn create_issue(&self, repo: &str, title: &str, body: &str) -> rbit_core::Issue {
        let input = NewIssueInput { title: title.into(), body: body.into(), ..NewIssueInput::default() };
        self.services.issues.create(&self.alice, &self.repo(repo).await, input).await.unwrap()
    }

    /// Delivers every event published so far, as the background subscriber would.
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

    async fn digests(&self, delivery: EmailDelivery, hour: Option<u8>) {
        self.email.set_preferences(&self.bob, EmailPreference::Participating, delivery, hour).await.unwrap();
    }

    async fn send(&self) -> DigestRun {
        let run = self.email.send_digests().await.unwrap();
        settle().await;
        run
    }

    fn mail_to(&self, name: &str) -> Vec<Mail> {
        let address = format!("{name}@example.com");
        self.outbox.sent().into_iter().filter(|mail| mail.to.as_str() == address).collect()
    }

    async fn pending(&self) -> u64 {
        self.database.email_digests().pending(self.bob.id()).await.unwrap()
    }
}

fn reader(clock: &Arc<AtomicI64>) -> impl Fn() -> Timestamp + Send + Sync + 'static {
    let clock = Arc::clone(clock);
    move || Timestamp(clock.load(Ordering::SeqCst))
}

async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn daily_digests_collect_notifications_grouped_by_repository() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let mut events = fixture.services.events.subscribe();
    fixture.digests(EmailDelivery::Daily, Some(8)).await;

    fixture.create_issue("tools", "Slow build", "@bob any idea?").await;
    fixture.create_issue("demo", "Crash", "@bob it crashes\n\nwhen   starting").await;
    let typo = fixture.create_issue("demo", "Typo", "No mention").await;
    fixture.deliver(&mut events).await;
    // Bob comments on the typo, then alice answers: bob hears about it as a commenter.
    let as_bob = fixture.services.timeline.open(Some(&fixture.bob), fixture.repo("demo").await, typo.number).await.unwrap();
    fixture.services.timeline.comment(&fixture.bob, &as_bob, "Where?").await.unwrap();
    let as_alice = fixture.services.timeline.open(Some(&fixture.alice), fixture.repo("demo").await, typo.number).await.unwrap();
    let answer = fixture.services.timeline.comment(&fixture.alice, &as_alice, "In the README").await.unwrap();
    fixture.deliver(&mut events).await;

    assert!(fixture.mail_to("bob").is_empty(), "digest users get no single emails");
    assert_eq!(fixture.pending().await, 3);
    assert_eq!(fixture.mail_to("alice").len(), 1, "immediate delivery is unchanged for everyone else");

    // Monday's period started at 08:00, before these were queued: they wait for Tuesday.
    assert_eq!(fixture.send().await, DigestRun::default());
    fixture.set_time(MONDAY + DAY + 8 * HOUR - 1);
    assert_eq!(fixture.send().await.sent, 0, "not a second early");

    fixture.set_time(MONDAY + DAY + 8 * HOUR);
    assert_eq!(fixture.send().await, DigestRun { sent: 1, ..DigestRun::default() });
    let sent = fixture.mail_to("bob");
    assert_eq!(sent.len(), 1);
    let digest = &sent[0];
    assert_eq!(digest.subject, "Your daily digest: 3 notifications in 2 repositories");
    let demo = digest.text.find("\nalice/demo\n").expect("a section per repository");
    let tools = digest.text.find("\nalice/tools\n").unwrap();
    assert!(demo < tools, "repositories in name order: {}", digest.text);
    assert!(digest.text.contains("Issue #1: Crash\n- alice opened issue #1.\n  @bob it crashes when starting\n"), "{}", digest.text);
    assert!(digest.text.contains(&format!("https://git.example.com/alice/demo/issues/2#issuecomment-{}", answer.id.0)));
    assert!(digest.text.contains("alice commented on issue #2."));
    assert!(digest.text.contains("https://git.example.com/alice/tools/issues/1\n"));
    let unsubscribe = digest.headers.one_click_unsubscribe.clone().expect("the same one-click unsubscribe");
    assert!(unsubscribe.starts_with(&format!("https://git.example.com/email/unsubscribe/{}.", fixture.bob.id().0)));
    assert!(digest.text.contains(&unsubscribe));
    assert_eq!(digest.headers.message_id, Some(format!("<digest/{}/{}@git.example.com>", fixture.bob.id().0, MONDAY + DAY + 8 * HOUR)));
    assert_eq!(digest.headers.in_reply_to, None);
    assert_eq!(fixture.pending().await, 0);

    // Later runs in the same period send nothing more.
    fixture.set_time(MONDAY + DAY + 20 * HOUR);
    assert_eq!(fixture.send().await, DigestRun::default());
    assert_eq!(fixture.mail_to("bob").len(), 1);
}

#[tokio::test]
async fn the_default_hour_applies_until_users_choose_one() {
    let fixture = Fixture::new(Visibility::Public, |config| config.notifications.digest_hour = 6).await;
    let mut events = fixture.services.events.subscribe();
    fixture.digests(EmailDelivery::Daily, None).await;
    let settings = fixture.email.settings(&fixture.bob).await.unwrap();
    assert_eq!((settings.delivery, settings.digest_hour, settings.default_digest_hour), (EmailDelivery::Daily, None, 6));

    fixture.create_issue("demo", "Crash", "@bob").await;
    fixture.deliver(&mut events).await;
    fixture.set_time(MONDAY + DAY + 6 * HOUR);
    assert_eq!(fixture.send().await.sent, 1, "sent at the instance's hour");
    assert!(fixture.email.set_preferences(&fixture.bob, EmailPreference::All, EmailDelivery::Daily, Some(24)).await.is_err());
}

#[tokio::test]
async fn weekly_digests_go_out_on_monday() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let mut events = fixture.services.events.subscribe();
    fixture.digests(EmailDelivery::Weekly, Some(8)).await;
    fixture.set_time(MONDAY + DAY + 12 * HOUR);
    fixture.create_issue("demo", "Crash", "@bob").await;
    fixture.deliver(&mut events).await;

    for (seconds, label) in [
        (MONDAY + 2 * DAY + 8 * HOUR, "Wednesday"),
        (MONDAY + 6 * DAY + 23 * HOUR, "Sunday night"),
        (MONDAY + 7 * DAY + 7 * HOUR, "Monday morning"),
    ] {
        fixture.set_time(seconds);
        assert_eq!(fixture.send().await.sent, 0, "{label}");
    }
    fixture.set_time(MONDAY + 7 * DAY + 8 * HOUR);
    assert_eq!(fixture.send().await.sent, 1);
    assert_eq!(fixture.mail_to("bob")[0].subject, "Your weekly digest: 1 notification in 1 repository");
    assert_eq!(period_start(EmailDelivery::Weekly, 8, Timestamp(MONDAY + 7 * DAY + 9 * HOUR)), Timestamp(MONDAY + 7 * DAY + 8 * HOUR));
}

#[tokio::test]
async fn threads_read_in_the_app_are_left_out() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let mut events = fixture.services.events.subscribe();
    fixture.digests(EmailDelivery::Daily, Some(8)).await;
    let read = fixture.create_issue("demo", "Crash", "@bob").await;
    fixture.create_issue("tools", "Slow", "@bob").await;
    fixture.deliver(&mut events).await;
    fixture.notifications.mark_thread_read(fixture.bob.id(), read.id).await;

    fixture.set_time(MONDAY + DAY + 8 * HOUR);
    assert_eq!(fixture.send().await, DigestRun { sent: 1, skipped_read: 1, ..DigestRun::default() });
    let digest = &fixture.mail_to("bob")[0];
    assert_eq!(digest.subject, "Your daily digest: 1 notification in 1 repository");
    assert!(!digest.text.contains("alice/demo"), "{}", digest.text);

    // When everything was read there is no email at all.
    let other = fixture.create_issue("demo", "Another", "@bob").await;
    fixture.deliver(&mut events).await;
    fixture.notifications.mark_thread_read(fixture.bob.id(), other.id).await;
    fixture.set_time(MONDAY + 2 * DAY + 8 * HOUR);
    assert_eq!(fixture.send().await, DigestRun { skipped_read: 1, ..DigestRun::default() });
    assert_eq!(fixture.mail_to("bob").len(), 1);
}

#[tokio::test]
async fn nothing_is_sent_twice_after_a_restart() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let mut events = fixture.services.events.subscribe();
    fixture.digests(EmailDelivery::Daily, Some(8)).await;
    fixture.create_issue("demo", "Crash", "@bob").await;
    fixture.deliver(&mut events).await;
    let due = MONDAY + DAY + 8 * HOUR;
    fixture.set_time(due);
    assert_eq!(fixture.send().await.sent, 1);

    // A restarted server has nothing in memory, only the database.
    let restarted = Services::new(Arc::clone(&fixture.config), &fixture.database, &fixture.kv).await.unwrap();
    let outbox = MemoryMailer::new();
    let email = restarted.notification_email.with_mail(mail_service(&outbox)).with_clock(reader(&fixture.clock));
    assert_eq!(email.send_digests().await.unwrap(), DigestRun::default());

    // A digest claimed just before a crash (sent or not) is not sent again either.
    fixture.create_issue("demo", "Again", "@bob").await;
    fixture.deliver(&mut events).await;
    let next = due + DAY;
    let claimed = fixture.database.email_digests().claim(fixture.bob.id(), Timestamp(next), Timestamp(next)).await.unwrap();
    assert!(matches!(claimed, Claim::Claimed { .. }), "{claimed:?}");
    fixture.set_time(next + HOUR);
    assert_eq!(email.send_digests().await.unwrap(), DigestRun::default());
    settle().await;
    assert_eq!(outbox.sent(), []);
    assert_eq!(fixture.mail_to("bob").len(), 1);
}

#[tokio::test]
async fn digests_recheck_access_and_preferences() {
    let fixture = Fixture::new(Visibility::Private, |_| {}).await;
    let mut events = fixture.services.events.subscribe();
    let repo = fixture.repo("demo").await;
    fixture.services.collaborators.add(&fixture.alice, &repo, "bob", Permission::Read).await.unwrap();
    fixture.digests(EmailDelivery::Daily, Some(8)).await;
    fixture.create_issue("demo", "Secret", "@bob").await;
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.pending().await, 1);

    // Bob loses access before the digest is due: nothing about the repository is sent.
    fixture.services.collaborators.remove(&fixture.alice, &repo, "bob").await.unwrap();
    fixture.set_time(MONDAY + DAY + 8 * HOUR);
    assert_eq!(fixture.send().await.sent, 0);
    assert_eq!(fixture.mail_to("bob"), []);
    assert_eq!(fixture.pending().await, 0, "and nothing waits for later");

    // Unsubscribing drops what is queued.
    fixture.services.collaborators.add(&fixture.alice, &repo, "bob", Permission::Read).await.unwrap();
    fixture.create_issue("demo", "Another", "@bob").await;
    fixture.deliver(&mut events).await;
    fixture.email.set_preference(&fixture.bob, EmailPreference::Off).await.unwrap();
    fixture.set_time(MONDAY + 2 * DAY + 8 * HOUR);
    assert_eq!(fixture.send().await, DigestRun { empty: 1, ..DigestRun::default() }, "emails off: the queue is dropped");
    assert_eq!(fixture.mail_to("bob"), []);
}

#[tokio::test]
async fn switching_back_to_immediate_sends_what_was_queued_once() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let mut events = fixture.services.events.subscribe();
    fixture.digests(EmailDelivery::Weekly, Some(8)).await;
    fixture.create_issue("demo", "Queued", "@bob").await;
    fixture.deliver(&mut events).await;
    fixture.digests(EmailDelivery::Immediate, Some(8)).await;
    fixture.create_issue("demo", "Immediate", "@bob").await;
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.mail_to("bob").len(), 1, "new notifications are mailed at once again");
    assert_eq!(fixture.pending().await, 1);

    fixture.set_time(MONDAY + DAY + 8 * HOUR);
    assert_eq!(fixture.send().await.sent, 1, "the queue goes out with the next daily period");
    let sent = fixture.mail_to("bob");
    assert_eq!(sent.len(), 2);
    assert!(sent[1].text.contains("Issue #1: Queued"), "{}", sent[1].text);
    assert!(!sent[1].text.contains("Immediate"));
}

#[tokio::test]
async fn nothing_is_sent_without_mail() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let mut events = fixture.services.events.subscribe();
    fixture.digests(EmailDelivery::Daily, Some(8)).await;
    fixture.create_issue("demo", "Crash", "@bob").await;
    fixture.deliver(&mut events).await;

    let disabled = fixture.email.with_mail(MailService::with_mailer(Arc::new(DisabledMailer), Duration::from_secs(1)));
    fixture.set_time(MONDAY + DAY + 8 * HOUR);
    assert_eq!(disabled.send_digests().await.unwrap(), DigestRun::default());
    assert_eq!(fixture.pending().await, 1, "nothing is claimed while mail is disabled");
    assert_eq!(fixture.database.email_digests().state(fixture.bob.id(), Timestamp(MONDAY + DAY + 8 * HOUR)).await.unwrap(), None);
}
