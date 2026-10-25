//! Email digests against a real, migrated `SQLite` database: delivery settings, queueing,
//! claiming one digest per user and period, skipping threads read in the app, and pruning.

use rbit_core::{
    AccountId, IssueId, IssueKind, IssueText, IssueTitle, RepoId, Timestamp, User, Visibility,
    config::{EmailDelivery, EmailPreference},
};
use rbit_storage::{
    Database, NewRepository, NewUser,
    sqlite::{
        email_digests::{Claim, ClaimedDigest, DigestCandidate, DigestState, NewDigestItem},
        issues::NewIssue,
        notification_email::DeliverySettings,
        notifications::{NotificationReason, NotificationRecipient},
    },
};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    alice: User,
    bob: User,
    repo: RepoId,
    first: IssueId,
    second: IssueId,
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

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let alice = user(&database, "alice").await;
        let bob = user(&database, "bob").await;
        let new = NewRepository {
            owner_id: alice.id(),
            name: "demo".parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_owned(),
        };
        let repo = database.repositories().create(new).await.unwrap().id;
        let mut issues = Vec::new();
        for title in ["Crash", "Typo"] {
            let new = NewIssue {
                repo,
                kind: IssueKind::Issue,
                title: IssueTitle::parse(title).unwrap(),
                body: IssueText::default(),
                author: alice.id(),
                at: Timestamp(1),
            };
            issues.push(database.issues().create(&new).await.unwrap().id);
        }
        Self { _dir: dir, database, alice, bob, repo, first: issues[0], second: issues[1] }
    }

    /// An unread inbox row for `user` about `issue`, as the fan-out writes it.
    async fn notify(&self, user: AccountId, issue: IssueId) {
        let recipient = NotificationRecipient { user, reason: NotificationReason::Mention, participation: None };
        self.database.notifications().notify_many(issue, self.repo, Some(self.alice.id()), Timestamp(1), &[recipient]).await.unwrap();
    }

    fn item(&self, account: AccountId, issue: IssueId, headline: &str, at: i64) -> NewDigestItem {
        NewDigestItem {
            account,
            repo: self.repo,
            issue,
            reason: NotificationReason::Mention,
            headline: headline.to_owned(),
            excerpt: String::new(),
            anchor: Some("issuecomment-1".to_owned()),
            created_at: Timestamp(at),
        }
    }
}

/// The digest of a successful claim, with how many read items it left out.
fn expect_claimed(claim: Claim) -> (ClaimedDigest, u64) {
    match claim {
        Claim::Claimed { digest, skipped_read } => (digest, skipped_read),
        other => panic!("expected a claimed digest, got {other:?}"),
    }
}

#[tokio::test]
async fn delivery_settings_default_to_immediate_and_survive_unsubscribing() {
    let fixture = Fixture::new().await;
    let store = fixture.database.notification_email();
    let bob = fixture.bob.id();
    assert_eq!(store.delivery(bob).await.unwrap(), DeliverySettings { delivery: EmailDelivery::Immediate, digest_hour: None });

    let weekly = DeliverySettings { delivery: EmailDelivery::Weekly, digest_hour: Some(17) };
    store.save(bob, EmailPreference::All, weekly, Timestamp(5)).await.unwrap();
    assert_eq!(store.delivery(bob).await.unwrap(), weekly);
    assert_eq!(store.preference(bob).await.unwrap(), Some(EmailPreference::All));

    // The one-click unsubscribe only turns emails off.
    store.set_preference(bob, EmailPreference::Off, Timestamp(6)).await.unwrap();
    assert_eq!(store.delivery(bob).await.unwrap(), weekly);
    // A preference stored before digests existed keeps immediate delivery.
    store.set_preference(fixture.alice.id(), EmailPreference::All, Timestamp(7)).await.unwrap();
    assert_eq!(store.delivery(fixture.alice.id()).await.unwrap().delivery, EmailDelivery::Immediate);
}

#[tokio::test]
async fn pending_users_are_listed_with_their_settings() {
    let fixture = Fixture::new().await;
    let digests = fixture.database.email_digests();
    let (alice, bob) = (fixture.alice.id(), fixture.bob.id());
    assert_eq!(digests.candidates(AccountId(0), 10).await.unwrap(), []);

    let daily = DeliverySettings { delivery: EmailDelivery::Daily, digest_hour: Some(6) };
    fixture.database.notification_email().save(bob, EmailPreference::Participating, daily, Timestamp(1)).await.unwrap();
    digests
        .queue(&[
            fixture.item(bob, fixture.first, "one", 10),
            fixture.item(bob, fixture.second, "two", 11),
            fixture.item(alice, fixture.first, "three", 12),
        ])
        .await
        .unwrap();
    assert_eq!(digests.pending(bob).await.unwrap(), 2);

    let all = digests.candidates(AccountId(0), 10).await.unwrap();
    assert_eq!(
        all,
        [
            DigestCandidate { account: alice, delivery: EmailDelivery::Immediate, digest_hour: None },
            DigestCandidate { account: bob, delivery: EmailDelivery::Daily, digest_hour: Some(6) },
        ],
        "one row per user, in id order"
    );
    let first_page = digests.candidates(AccountId(0), 1).await.unwrap();
    assert_eq!(first_page.len(), 1);
    assert_eq!(digests.candidates(first_page[0].account, 10).await.unwrap()[0].account, bob, "pages continue after the last id");
}

#[tokio::test]
async fn each_period_is_claimed_once_with_the_items_queued_before_it() {
    let fixture = Fixture::new().await;
    let digests = fixture.database.email_digests();
    let bob = fixture.bob.id();
    fixture.notify(bob, fixture.first).await;
    fixture.notify(bob, fixture.second).await;
    digests
        .queue(&[
            fixture.item(bob, fixture.second, "later", 20),
            fixture.item(bob, fixture.first, "earlier", 10),
            fixture.item(bob, fixture.first, "next period", 100),
        ])
        .await
        .unwrap();

    let (claimed, skipped_read) = expect_claimed(digests.claim(bob, Timestamp(50), Timestamp(60)).await.unwrap());
    assert_eq!((claimed.items, skipped_read, claimed.period), (2, 0, Timestamp(50)), "the first claim wins");
    assert_eq!(digests.state(bob, Timestamp(50)).await.unwrap(), Some(DigestState::Claimed));
    let items = digests.items(claimed.id).await.unwrap();
    let headlines: Vec<_> = items.iter().map(|item| item.headline.as_str()).collect();
    assert_eq!(headlines, ["earlier", "later"], "grouped by thread, issue #1 first");
    assert_eq!((items[0].owner.as_str(), items[0].repo_name.as_str(), items[0].title.as_str()), ("alice", "demo", "Crash"));
    assert_eq!(items[0].anchor.as_deref(), Some("issuecomment-1"));
    assert_eq!(digests.pending(bob).await.unwrap(), 1, "the item queued after the period start waits");

    // A second sender, or the same one after a restart, cannot claim the period again.
    assert_eq!(digests.claim(bob, Timestamp(50), Timestamp(61)).await.unwrap(), Claim::Taken);
    digests.finish(claimed.id, DigestState::Sent, Timestamp(62)).await.unwrap();
    assert_eq!(digests.state(bob, Timestamp(50)).await.unwrap(), Some(DigestState::Sent));
    assert_eq!(digests.claim(bob, Timestamp(50), Timestamp(63)).await.unwrap(), Claim::Taken, "not even once it is sent");

    let (next, _) = expect_claimed(digests.claim(bob, Timestamp(150), Timestamp(160)).await.unwrap());
    assert_eq!(next.items, 1);
    assert_eq!(digests.pending(bob).await.unwrap(), 0);
}

#[tokio::test]
async fn threads_read_in_the_app_are_left_out() {
    let fixture = Fixture::new().await;
    let digests = fixture.database.email_digests();
    let bob = fixture.bob.id();
    fixture.notify(bob, fixture.first).await;
    fixture.notify(bob, fixture.second).await;
    digests.queue(&[fixture.item(bob, fixture.first, "read", 10), fixture.item(bob, fixture.second, "unread", 11)]).await.unwrap();
    assert!(fixture.database.notifications().mark_issue_read(bob, fixture.first).await.unwrap());

    let (digest, skipped_read) = expect_claimed(digests.claim(bob, Timestamp(50), Timestamp(60)).await.unwrap());
    assert_eq!((digest.items, skipped_read), (1, 1));
    assert_eq!(digests.items(digest.id).await.unwrap()[0].headline, "unread");

    // When everything was read, no digest is recorded and nothing waits any more.
    digests.queue(&[fixture.item(bob, fixture.second, "read too", 70)]).await.unwrap();
    fixture.database.notifications().mark_issue_read(bob, fixture.second).await.unwrap();
    assert_eq!(digests.claim(bob, Timestamp(100), Timestamp(110)).await.unwrap(), Claim::Nothing { skipped_read: 1 });
    assert_eq!(digests.state(bob, Timestamp(100)).await.unwrap(), None);
    assert_eq!(digests.pending(bob).await.unwrap(), 0);
    assert_eq!(digests.candidates(AccountId(0), 10).await.unwrap(), []);
}

#[tokio::test]
async fn pruning_removes_old_digests_and_items() {
    let fixture = Fixture::new().await;
    let digests = fixture.database.email_digests();
    let bob = fixture.bob.id();
    fixture.notify(bob, fixture.first).await;
    digests.queue(&[fixture.item(bob, fixture.first, "sent", 10), fixture.item(bob, fixture.first, "stale", 20)]).await.unwrap();
    let (claimed, _) = expect_claimed(digests.claim(bob, Timestamp(15), Timestamp(16)).await.unwrap());
    digests.queue(&[fixture.item(bob, fixture.first, "fresh", 500)]).await.unwrap();

    assert_eq!(digests.prune(Timestamp(100)).await.unwrap(), 2, "the old digest and the old unclaimed item");
    assert!(digests.items(claimed.id).await.unwrap().is_empty(), "items go with their digest");
    assert_eq!(digests.pending(bob).await.unwrap(), 1);
}
