//! Webhook and delivery queue persistence against a real, migrated SQLite database.

use std::collections::HashSet;

use rbit_core::{
    AccountId, RepoId, Timestamp, User, Visibility,
    webhook::{DeliveryStatus, WebhookEvent, WebhookId, WebhookOwner, WebhookUrl},
};
use rbit_storage::{
    Database, NewRepository, NewUser,
    sqlite::{
        orgs::NewOrganization,
        webhooks::{AttemptOutcome, AttemptRecord, NewDelivery, NewWebhook, SecretUpdate, WebhookStore, WebhookUpdate},
    },
};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    alice: User,
    repo: RepoId,
    org: AccountId,
    org_repo: RepoId,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let alice = database
            .accounts()
            .create_user(NewUser {
                name: "alice".parse().unwrap(),
                email: "alice@example.com".parse().unwrap(),
                display_name: None,
                password_hash: "not-a-real-hash".to_owned(),
                is_admin: false,
            })
            .await
            .unwrap();
        let org = database
            .orgs()
            .create(NewOrganization { name: "acme".parse().unwrap(), display_name: None, description: None, creator: alice.id() })
            .await
            .unwrap()
            .account
            .id;
        let repo = create_repo(&database, alice.id(), "demo").await;
        let org_repo = create_repo(&database, org, "tools").await;
        Self { _dir: dir, database, alice, repo, org, org_repo }
    }

    fn store(&self) -> WebhookStore {
        self.database.webhooks()
    }

    fn hook(&self, owner: WebhookOwner, events: &[WebhookEvent]) -> NewWebhook {
        NewWebhook {
            owner,
            url: WebhookUrl::parse("https://hooks.example.com/rbit").unwrap(),
            secret: Some("s3cret".to_owned()),
            events: events.to_vec(),
            active: true,
            created_by: Some(self.alice.id()),
        }
    }
}

async fn create_repo(database: &Database, owner: AccountId, name: &str) -> RepoId {
    database
        .repositories()
        .create(NewRepository {
            owner_id: owner,
            name: name.parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_owned(),
        })
        .await
        .unwrap()
        .id
}

fn delivery(hook: WebhookId, guid: &str) -> NewDelivery {
    NewDelivery {
        hook_id: hook,
        guid: guid.to_owned(),
        event: WebhookEvent::Push,
        action: None,
        payload: r#"{"ref":"refs/heads/main"}"#.to_owned(),
    }
}

fn attempt(status: Option<u16>) -> AttemptRecord {
    AttemptRecord {
        at: Timestamp::now(),
        request_headers: vec![("X-Rbit-Event".to_owned(), "push".to_owned())],
        response_status: status,
        response_headers: vec![("Content-Type".to_owned(), "text/plain".to_owned())],
        response_body: Some("ok".to_owned()),
        duration_ms: 12,
        error: status.is_none().then(|| "connection refused".to_owned()),
    }
}

#[tokio::test]
async fn hooks_are_scoped_to_their_owner_and_secrets_are_write_only() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let repo = WebhookOwner::Repository(fixture.repo);
    let other = WebhookOwner::Repository(fixture.org_repo);

    let hook = store.create(fixture.hook(repo, &[WebhookEvent::Push, WebhookEvent::Release])).await.unwrap();
    assert_eq!(hook.owner, repo);
    assert_eq!(hook.events, [WebhookEvent::Push, WebhookEvent::Release]);
    assert!(hook.has_secret && hook.active);
    assert_eq!(hook.created_by, Some(fixture.alice.id()));
    assert!(hook.last_delivery.is_none());

    assert_eq!(store.count_for(repo).await.unwrap(), 1);
    assert_eq!(store.count_for(other).await.unwrap(), 0);
    assert_eq!(store.list_for(repo).await.unwrap(), std::slice::from_ref(&hook));
    assert!(store.find(other, hook.id).await.unwrap().is_none(), "another repository cannot reach the hook");
    assert!(store.find(WebhookOwner::Organization(fixture.org), hook.id).await.unwrap().is_none());

    let update = |secret| WebhookUpdate {
        url: WebhookUrl::parse("https://hooks.example.com/v2").unwrap(),
        secret,
        events: vec![WebhookEvent::Fork],
        active: false,
    };
    assert!(store.update(other, hook.id, update(SecretUpdate::Clear)).await.unwrap().is_none());
    let kept = store.update(repo, hook.id, update(SecretUpdate::Keep)).await.unwrap().unwrap();
    assert!(kept.has_secret && !kept.active);
    assert_eq!((kept.url.as_str(), kept.events.as_slice()), ("https://hooks.example.com/v2", [WebhookEvent::Fork].as_slice()));
    assert!(!store.update(repo, hook.id, update(SecretUpdate::Clear)).await.unwrap().unwrap().has_secret);
    assert!(store.update(repo, hook.id, update(SecretUpdate::Set("new".to_owned()))).await.unwrap().unwrap().has_secret);

    assert!(!store.delete(other, hook.id).await.unwrap());
    assert!(store.delete(repo, hook.id).await.unwrap());
    assert!(store.find(repo, hook.id).await.unwrap().is_none());
}

#[tokio::test]
async fn subscriptions_cover_repository_and_organisation_hooks() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let repo_hook = store.create(fixture.hook(WebhookOwner::Repository(fixture.org_repo), &[WebhookEvent::Push])).await.unwrap();
    let org_hook =
        store.create(fixture.hook(WebhookOwner::Organization(fixture.org), &[WebhookEvent::Push, WebhookEvent::Repository])).await.unwrap();
    let elsewhere = store.create(fixture.hook(WebhookOwner::Repository(fixture.repo), &[WebhookEvent::Push])).await.unwrap();
    let mut inactive = fixture.hook(WebhookOwner::Organization(fixture.org), &[WebhookEvent::Push]);
    inactive.active = false;
    store.create(inactive).await.unwrap();

    let push = store.subscribed(Some(fixture.org_repo), &[fixture.org], WebhookEvent::Push).await.unwrap();
    assert_eq!(push, [repo_hook.id, org_hook.id]);
    let repository = store.subscribed(Some(fixture.org_repo), &[fixture.org], WebhookEvent::Repository).await.unwrap();
    assert_eq!(repository, [org_hook.id]);
    assert_eq!(store.subscribed(None, &[fixture.org], WebhookEvent::Repository).await.unwrap(), [org_hook.id], "deleted repositories");
    assert_eq!(store.subscribed(Some(fixture.repo), &[], WebhookEvent::Push).await.unwrap(), [elsewhere.id]);
    assert_eq!(store.subscribed(Some(fixture.repo), &[], WebhookEvent::Release).await.unwrap(), Vec::<WebhookId>::new());
}

#[tokio::test]
async fn deliveries_are_claimed_once_retried_and_recorded() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let owner = WebhookOwner::Repository(fixture.repo);
    let hook = store.create(fixture.hook(owner, &[WebhookEvent::Push])).await.unwrap();
    let ids = store.enqueue(vec![delivery(hook.id, "guid-1"), delivery(hook.id, "guid-2")]).await.unwrap();
    assert!(store.enqueue(vec![delivery(hook.id, "guid-1")]).await.is_err(), "guids are unique");

    let now = Timestamp::now();
    let claimed = store.claim_due(now, 1).await.unwrap();
    assert_eq!(claimed.len(), 1);
    let first = &claimed[0];
    assert_eq!((first.id, first.guid.as_str(), first.attempts), (ids[0], "guid-1", 0));
    assert_eq!((first.url.as_str(), first.secret.as_deref()), ("https://hooks.example.com/rbit", Some("s3cret")));
    assert_eq!(store.find_delivery(hook.id, first.id).await.unwrap().unwrap().status, DeliveryStatus::Delivering);

    // A failed attempt goes back to the queue, due later.
    let later = Timestamp(now.seconds() + 60);
    store.record_attempt(first.id, attempt(Some(500)), AttemptOutcome::Retry(later)).await.unwrap();
    let retrying = store.find_delivery(hook.id, first.id).await.unwrap().unwrap();
    assert_eq!((retrying.status, retrying.attempts, retrying.next_attempt_at), (DeliveryStatus::Pending, 1, Some(later)));
    assert_eq!(retrying.response_status, Some(500));
    assert_eq!(retrying.request_headers, [("X-Rbit-Event".to_owned(), "push".to_owned())]);
    assert_eq!(retrying.response_body.as_deref(), Some("ok"));

    let second = store.claim_due(now, 10).await.unwrap();
    assert_eq!(second.iter().map(|d| d.id).collect::<Vec<_>>(), [ids[1]], "the retry is not due yet");
    store.record_attempt(ids[1], attempt(None), AttemptOutcome::Failed).await.unwrap();

    let again = store.claim_due(later, 10).await.unwrap();
    assert_eq!(again.iter().map(|d| (d.id, d.attempts)).collect::<Vec<_>>(), [(ids[0], 1)]);
    store.record_attempt(ids[0], attempt(Some(204)), AttemptOutcome::Succeeded).await.unwrap();

    let listed = store.list_deliveries(hook.id, 10, 0).await.unwrap();
    assert_eq!(
        listed.iter().map(|d| (d.guid.as_str(), d.status)).collect::<Vec<_>>(),
        [("guid-2", DeliveryStatus::Failed), ("guid-1", DeliveryStatus::Succeeded)]
    );
    assert_eq!(listed[0].error.as_deref(), Some("connection refused"));
    assert_eq!(listed[1].attempts, 2);
    assert_eq!(store.count_deliveries(hook.id).await.unwrap(), 2);
    assert_eq!(store.list_deliveries(hook.id, 1, 1).await.unwrap()[0].guid, "guid-1");

    let last = store.find(owner, hook.id).await.unwrap().unwrap().last_delivery.unwrap();
    assert_eq!((last.status, last.response_status), (DeliveryStatus::Failed, None), "the newest delivery is the latest");
}

#[tokio::test]
async fn two_workers_never_claim_the_same_delivery() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let hook = store.create(fixture.hook(WebhookOwner::Repository(fixture.repo), &[WebhookEvent::Push])).await.unwrap();
    let queued: Vec<_> = (0..60).map(|n| delivery(hook.id, &format!("guid-{n}"))).collect();
    let ids: HashSet<_> = store.enqueue(queued).await.unwrap().into_iter().collect();

    let worker = |store: WebhookStore| {
        tokio::spawn(async move {
            let mut mine = Vec::new();
            loop {
                let batch = store.claim_due(Timestamp::now(), 3).await.unwrap();
                if batch.is_empty() {
                    break mine;
                }
                mine.extend(batch.into_iter().map(|claimed| claimed.id));
            }
        })
    };
    let (first, second) = tokio::join!(worker(store.clone()), worker(store.clone()));
    let (first, second) = (first.unwrap(), second.unwrap());
    let mut seen = HashSet::new();
    for id in first.iter().chain(&second) {
        assert!(seen.insert(*id), "{id:?} was claimed twice");
    }
    assert_eq!(seen, ids, "every delivery was claimed");
}

#[tokio::test]
async fn interrupted_deliveries_return_to_the_queue() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let hook = store.create(fixture.hook(WebhookOwner::Repository(fixture.repo), &[WebhookEvent::Push])).await.unwrap();
    store.enqueue(vec![delivery(hook.id, "a"), delivery(hook.id, "b")]).await.unwrap();
    let claimed = store.claim_due(Timestamp::now(), 2).await.unwrap();

    store.release(claimed[0].id).await.unwrap();
    let after_release = store.find_delivery(hook.id, claimed[0].id).await.unwrap().unwrap();
    assert_eq!((after_release.status, after_release.attempts), (DeliveryStatus::Pending, 0), "no attempt is counted");
    assert_eq!(store.release_interrupted().await.unwrap(), 1);
    assert_eq!(store.claim_due(Timestamp::now(), 10).await.unwrap().len(), 2);
}

#[tokio::test]
async fn pruning_removes_only_finished_deliveries_before_the_cutoff() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let hook = store.create(fixture.hook(WebhookOwner::Repository(fixture.repo), &[WebhookEvent::Push])).await.unwrap();
    let ids = store.enqueue(vec![delivery(hook.id, "done"), delivery(hook.id, "failed"), delivery(hook.id, "waiting")]).await.unwrap();
    store.claim_due(Timestamp::now(), 2).await.unwrap();
    store.record_attempt(ids[0], attempt(Some(200)), AttemptOutcome::Succeeded).await.unwrap();
    store.record_attempt(ids[1], attempt(Some(410)), AttemptOutcome::Failed).await.unwrap();

    assert_eq!(store.prune(Timestamp(Timestamp::now().seconds() - 60)).await.unwrap(), 0, "nothing is old enough");
    assert_eq!(store.prune(Timestamp(Timestamp::now().seconds() + 60)).await.unwrap(), 2);
    let left: Vec<_> = store.list_deliveries(hook.id, 10, 0).await.unwrap().into_iter().map(|d| d.guid).collect();
    assert_eq!(left, ["waiting"]);
}

#[tokio::test]
async fn deleting_the_repository_removes_its_hooks_and_deliveries() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let owner = WebhookOwner::Repository(fixture.repo);
    let hook = store.create(fixture.hook(owner, &[WebhookEvent::Push])).await.unwrap();
    store.enqueue(vec![delivery(hook.id, "a")]).await.unwrap();

    fixture.database.repositories().delete(fixture.repo).await.unwrap();
    assert_eq!(store.list_for(owner).await.unwrap().len(), 0);
    assert_eq!(store.count_deliveries(hook.id).await.unwrap(), 0);
    assert!(store.claim_due(Timestamp::now(), 10).await.unwrap().is_empty());
}
