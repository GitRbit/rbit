//! Webhook management and deliveries against real stores, real bare repositories and a
//! local HTTP receiver (the test configuration allows private addresses).

use std::{
    collections::VecDeque,
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rbit_core::{
    Config, OrgRole, Timestamp, User, Visibility,
    webhook::{DeliveryStatus, WebhookEvent, WebhookId, WebhookOwner, WebhookUrl},
};
use rbit_services::{
    CreateRepository, DomainEvent, RepoContext, ServiceError, Services,
    events::RefUpdate,
    forks::ForkInput,
    orgs::CreateOrganization,
    releases::ReleaseInput,
    webhooks::{HookTarget, MAX_HOOKS_PER_OWNER, WebhookInput, WebhookService, delivery::signature},
};
use rbit_storage::{
    Database, KvStore, NewUser,
    sqlite::webhooks::{NewWebhook, SecretUpdate},
};
use serde_json::Value;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

// ------------------------------------------------------------------ a tiny receiver

/// One request the receiver accepted.
#[derive(Debug, Clone)]
struct Received {
    headers: Vec<(String, String)>,
    body: String,
}

impl Received {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap()
    }
}

/// An HTTP/1.1 endpoint on 127.0.0.1 that records requests and answers with scripted
/// statuses (then 200).
#[derive(Debug, Clone)]
struct Receiver {
    url: String,
    received: Arc<Mutex<Vec<Received>>>,
}

impl Receiver {
    async fn start(statuses: &[u16]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/hook", listener.local_addr().unwrap());
        let received = Arc::new(Mutex::new(Vec::new()));
        let script = Arc::new(Mutex::new(statuses.iter().copied().collect::<VecDeque<_>>()));
        let log = Arc::clone(&received);
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else { return };
                let log = Arc::clone(&log);
                let script = Arc::clone(&script);
                tokio::spawn(async move {
                    let Some(request) = read_request(&mut socket).await else { return };
                    log.lock().unwrap().push(request);
                    let status = script.lock().unwrap().pop_front().unwrap_or(200);
                    let body = format!("status {status}");
                    let response = format!(
                        "HTTP/1.1 {status} Scripted\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        Self { url, received }
    }

    fn received(&self) -> Vec<Received> {
        self.received.lock().unwrap().clone()
    }

    /// Waits until `count` requests arrived.
    async fn wait_for(&self, count: usize) -> Vec<Received> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let received = self.received();
            if received.len() >= count {
                return received;
            }
            assert!(Instant::now() < deadline, "only {} of {count} deliveries arrived", received.len());
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> Option<Received> {
    let mut data = Vec::new();
    let mut buffer = [0_u8; 4096];
    let header_end = loop {
        let read = socket.read(&mut buffer).await.ok()?;
        if read == 0 {
            return None;
        }
        data.extend_from_slice(&buffer[..read]);
        if let Some(end) = data.windows(4).position(|window| window == b"\r\n\r\n") {
            break end;
        }
    };
    let head = String::from_utf8_lossy(&data[..header_end]).into_owned();
    let headers: Vec<(String, String)> = head
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .collect();
    let length: usize =
        headers.iter().find(|(name, _)| name.eq_ignore_ascii_case("content-length")).and_then(|(_, value)| value.parse().ok())?;
    let mut body = data[header_end + 4..].to_vec();
    while body.len() < length {
        let read = socket.read(&mut buffer).await.ok()?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&buffer[..read]);
    }
    Some(Received { headers, body: String::from_utf8_lossy(&body).into_owned() })
}

// ------------------------------------------------------------------ fixture

struct Fixture {
    _dir: tempfile::TempDir,
    config: Arc<Config>,
    database: Database,
    kv: KvStore,
    services: Services,
}

impl Fixture {
    async fn new(configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        "https://git.example.com".clone_into(&mut config.server.public_url);
        config.webhooks.allow_private_addresses = true;
        config.webhooks.max_attempts = 3;
        configure(&mut config);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        Self { _dir: dir, config, database, kv, services }
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

    async fn repo(&self, actor: &User, owner: &str, name: &str) -> RepoContext {
        let input = CreateRepository { name: name.to_owned(), description: String::new(), visibility: Visibility::Private };
        self.services.repositories.create_in(actor, owner, input).await.unwrap();
        self.services.repositories.open(Some(actor), owner, name).await.unwrap()
    }
}

fn input(url: &str, events: &[&str], secret: Option<&str>) -> WebhookInput {
    WebhookInput {
        url: url.to_owned(),
        events: events.iter().map(|&event| event.to_owned()).collect(),
        active: true,
        secret: secret.map_or(SecretUpdate::Keep, |secret| SecretUpdate::Set(secret.to_owned())),
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Pushes two commits to `main` of `repo` and returns the new tip.
fn push_two_commits(repo: &RepoContext) -> String {
    let work = tempfile::tempdir().unwrap();
    git(work.path(), &["init", "-q"]);
    std::fs::write(work.path().join("README.md"), "# Demo\n").unwrap();
    git(work.path(), &["add", "."]);
    git(work.path(), &["commit", "-q", "-m", "First"]);
    std::fs::write(work.path().join("main.rs"), "fn main() {}\n").unwrap();
    git(work.path(), &["add", "."]);
    git(work.path(), &["commit", "-q", "-m", "Second", "-m", "With a body."]);
    git(work.path(), &["push", "-q", repo.disk_path.to_str().unwrap(), "HEAD:refs/heads/main"]);
    git(work.path(), &["rev-parse", "HEAD"])
}

fn far_future() -> Timestamp {
    Timestamp(Timestamp::now().seconds() + 365 * 24 * 60 * 60)
}

// ------------------------------------------------------------------ tests

#[tokio::test]
async fn a_push_is_signed_delivered_and_retried_after_a_server_error() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let repo = fixture.repo(&alice, "alice", "demo").await;
    // The ping succeeds; the push fails once with a 500, then succeeds.
    let receiver = Receiver::start(&[200, 500]).await;
    let webhooks = &fixture.services.webhooks;

    let hook =
        webhooks.create(&alice, HookTarget::Repository(&repo), input(&receiver.url, &["push"], Some("It's a secret"))).await.unwrap();
    assert!(hook.has_secret);
    assert_eq!(webhooks.deliver_due(Timestamp::now()).await.unwrap(), 1);
    let ping = &receiver.received()[0];
    assert_eq!(ping.header("X-Rbit-Event"), Some("ping"));
    assert_eq!(ping.json()["hook_id"], hook.id.0);
    assert_eq!(ping.json()["hook"]["config"]["url"], receiver.url.as_str());
    assert_eq!(ping.json()["repository"]["full_name"], "alice/demo");
    assert!(!ping.body.contains("It's a secret"), "the secret never travels");

    let head = push_two_commits(&repo);
    let update = RefUpdate { name: "refs/heads/main".to_owned(), before: None, after: Some(head.clone()) };
    let event = DomainEvent::RepositoryPushed { repo: repo.repo.repo.id, pusher: Some(alice.id()), updates: vec![update] };
    assert_eq!(webhooks.handle_event(&event).await.unwrap(), 1);
    assert_eq!(webhooks.deliver_due(far_future()).await.unwrap(), 2, "one failed attempt and its retry");

    let requests = receiver.received();
    assert_eq!(requests.len(), 3);
    let push = &requests[2];
    assert_eq!(push.body, requests[1].body, "a retry sends the same body");
    assert_eq!(push.header("X-Rbit-Delivery"), requests[1].header("X-Rbit-Delivery"), "and the same delivery id");
    assert_eq!(push.header("X-Rbit-Event"), Some("push"));
    assert_eq!(push.header("X-GitHub-Event"), Some("push"));
    assert_eq!(push.header("Content-Type"), Some("application/json"));
    assert!(push.header("User-Agent").is_some_and(|agent| agent.starts_with("rbit-Hookshot/")));
    let expected = format!("sha256={}", signature("It's a secret", push.body.as_bytes()));
    assert_eq!(push.header("X-Rbit-Signature-256"), Some(expected.as_str()));
    assert_eq!(push.header("X-Hub-Signature-256"), Some(expected.as_str()));

    let payload = push.json();
    assert_eq!(payload["ref"], "refs/heads/main");
    assert_eq!(payload["before"], "0000000000000000000000000000000000000000");
    assert_eq!(payload["after"], head);
    assert_eq!(payload["created"], true);
    assert_eq!(payload["total_commits"], 2);
    let messages: Vec<_> = payload["commits"].as_array().unwrap().iter().map(|commit| commit["message"].as_str().unwrap()).collect();
    assert_eq!(messages, ["First", "Second\n\nWith a body."]);
    assert_eq!(payload["head_commit"]["id"], head);
    assert_eq!(payload["pusher"]["name"], "alice");
    assert_eq!(payload["sender"]["login"], "alice");
    assert_eq!(payload["repository"]["html_url"], "https://git.example.com/alice/demo");

    // The log shows both deliveries; the push needed two attempts.
    let log = webhooks.deliveries(HookTarget::Repository(&repo), hook.id, 1, 10).await.unwrap();
    assert_eq!(log.total, 2);
    let push_delivery = &log.deliveries[0];
    assert_eq!((push_delivery.event, push_delivery.status, push_delivery.attempts), (WebhookEvent::Push, DeliveryStatus::Succeeded, 2));
    assert_eq!(push_delivery.response_status, Some(200));
    assert_eq!(push_delivery.response_body.as_deref(), Some("status 200"));
    assert!(push_delivery.request_headers.iter().any(|(name, value)| name == "X-Rbit-Signature-256" && *value == expected));
    assert!(!push_delivery.request_headers.iter().any(|(_, value)| value.contains("It's a secret")));
    let hook = webhooks.get(HookTarget::Repository(&repo), hook.id).await.unwrap();
    assert_eq!(hook.last_delivery.map(|last| last.status), Some(DeliveryStatus::Succeeded));
}

/// The events of a hook's delivery log, oldest first.
async fn logged_events(webhooks: &WebhookService, target: HookTarget<'_>, hook: WebhookId) -> Vec<WebhookEvent> {
    let mut events: Vec<_> = webhooks.deliveries(target, hook, 1, 100).await.unwrap().deliveries.iter().map(|d| d.event).collect();
    events.reverse();
    events
}

/// Publishes a push of `head` to `main` of `repo` on the bus.
fn publish_push(fixture: &Fixture, repo: &RepoContext, pusher: &User, head: &str) {
    let update = RefUpdate { name: "refs/heads/main".to_owned(), before: None, after: Some(head.to_owned()) };
    fixture.services.events.publish(DomainEvent::RepositoryPushed {
        repo: repo.repo.repo.id,
        pusher: Some(pusher.id()),
        updates: vec![update],
    });
}

#[tokio::test]
async fn a_hook_never_receives_events_that_happened_before_it_was_created() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let repo = fixture.repo(&alice, "alice", "demo").await;
    let receiver = Receiver::start(&[]).await;
    let webhooks = &fixture.services.webhooks;
    let target = HookTarget::Repository(&repo);
    let older = webhooks.create(&alice, target, input(&receiver.url, &["push"], None)).await.unwrap();

    // The subscriber's view of the bus, driven by hand: the push is published, then a hook
    // is added before the push is handled, as when the subscriber runs behind.
    let head = push_two_commits(&repo);
    let mut bus = fixture.services.events.subscribe_published();
    publish_push(&fixture, &repo, &alice, &head);
    let push = bus.recv().await.unwrap();
    let newer = webhooks.create(&alice, target, input(&receiver.url, &["push"], None)).await.unwrap();
    assert_eq!(webhooks.handle_published(&push).await.unwrap(), 1, "only the hook that existed at the push");

    // A push after both hooks exist reaches both.
    publish_push(&fixture, &repo, &alice, &head);
    assert_eq!(webhooks.handle_published(&bus.recv().await.unwrap()).await.unwrap(), 2);

    assert_eq!(logged_events(webhooks, target, older.id).await, [WebhookEvent::Ping, WebhookEvent::Push, WebhookEvent::Push]);
    assert_eq!(logged_events(webhooks, target, newer.id).await, [WebhookEvent::Ping, WebhookEvent::Push]);
}

#[tokio::test]
async fn hooks_stored_before_start_up_receive_events_and_new_ids_go_above_them() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let repo = fixture.repo(&alice, "alice", "demo").await;
    let receiver = Receiver::start(&[]).await;
    let stored = NewWebhook {
        id: Some(WebhookId(10)),
        owner: WebhookOwner::Repository(repo.repo.repo.id),
        url: WebhookUrl::parse(&receiver.url).unwrap(),
        secret: None,
        events: vec![WebhookEvent::Push],
        active: true,
        created_by: None,
    };
    let existing = fixture.database.webhooks().create(stored).await.unwrap();

    // As after a restart: the services start over a database that already has hook 10.
    let services = Services::new(Arc::clone(&fixture.config), &fixture.database, &fixture.kv).await.unwrap();
    let mut bus = services.events.subscribe_published();
    let head = push_two_commits(&repo);
    let update = RefUpdate { name: "refs/heads/main".to_owned(), before: None, after: Some(head) };
    services.events.publish(DomainEvent::RepositoryPushed { repo: repo.repo.repo.id, pusher: Some(alice.id()), updates: vec![update] });
    let push = bus.recv().await.unwrap();
    let created = services.webhooks.create(&alice, HookTarget::Repository(&repo), input(&receiver.url, &["push"], None)).await.unwrap();
    assert_eq!(created.id, WebhookId(11), "new ids continue after the stored ones");
    assert_eq!(services.webhooks.handle_published(&push).await.unwrap(), 1);
    let target = HookTarget::Repository(&repo);
    assert_eq!(logged_events(&services.webhooks, target, existing.id).await, [WebhookEvent::Push]);
    assert_eq!(logged_events(&services.webhooks, target, created.id).await, [WebhookEvent::Ping]);
}

#[tokio::test]
async fn deleting_or_deactivating_a_hook_stops_events_not_yet_queued() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let repo = fixture.repo(&alice, "alice", "demo").await;
    let receiver = Receiver::start(&[]).await;
    let webhooks = &fixture.services.webhooks;
    let target = HookTarget::Repository(&repo);
    let kept = webhooks.create(&alice, target, input(&receiver.url, &["push"], None)).await.unwrap();
    let deleted = webhooks.create(&alice, target, input(&receiver.url, &["push"], None)).await.unwrap();
    let paused = webhooks.create(&alice, target, input(&receiver.url, &["push"], None)).await.unwrap();

    // All three existed and were active when the push happened; by the time it is handled
    // one is deleted and one switched off.
    let head = push_two_commits(&repo);
    let mut bus = fixture.services.events.subscribe_published();
    publish_push(&fixture, &repo, &alice, &head);
    let push = bus.recv().await.unwrap();
    webhooks.delete(&alice, target, deleted.id).await.unwrap();
    webhooks.update(&alice, target, paused.id, WebhookInput { active: false, ..input(&receiver.url, &["push"], None) }).await.unwrap();
    assert_eq!(webhooks.handle_published(&push).await.unwrap(), 1, "only the hook still there and on");

    assert_eq!(logged_events(webhooks, target, kept.id).await, [WebhookEvent::Ping, WebhookEvent::Push]);
    assert_eq!(logged_events(webhooks, target, paused.id).await, [WebhookEvent::Ping]);
    assert!(matches!(webhooks.deliveries(target, deleted.id, 1, 10).await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn deliveries_give_up_after_max_attempts_and_can_be_redelivered() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let repo = fixture.repo(&alice, "alice", "demo").await;
    let receiver = Receiver::start(&[500, 502, 503]).await;
    let webhooks = &fixture.services.webhooks;
    let target = HookTarget::Repository(&repo);

    let hook = webhooks.create(&alice, target, input(&receiver.url, &["push"], None)).await.unwrap();
    assert_eq!(webhooks.deliver_due(far_future()).await.unwrap(), 3, "max_attempts is 3 in this test");
    let failed = webhooks.deliveries(target, hook.id, 1, 10).await.unwrap().deliveries.remove(0);
    assert_eq!((failed.status, failed.attempts, failed.response_status), (DeliveryStatus::Failed, 3, Some(503)));
    assert!(receiver.received().iter().all(|request| request.header("X-Rbit-Signature-256").is_none()), "no secret, no signature");

    let again = webhooks.redeliver(&alice, target, hook.id, failed.id).await.unwrap();
    assert_ne!(again.guid, failed.guid);
    assert_eq!((again.status, again.payload.as_str()), (DeliveryStatus::Pending, failed.payload.as_str()));
    webhooks.deliver_due(Timestamp::now()).await.unwrap();
    assert_eq!(webhooks.delivery(target, hook.id, again.id).await.unwrap().status, DeliveryStatus::Succeeded);
    assert_eq!(webhooks.deliveries(target, hook.id, 1, 1).await.unwrap().deliveries.len(), 1, "pages are bounded");
    assert_eq!(webhooks.deliveries(target, hook.id, 2, 1).await.unwrap().deliveries[0].id, failed.id);
}

#[tokio::test]
async fn organisation_hooks_fire_for_its_repositories_in_the_background() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let org = fixture
        .services
        .orgs
        .create(&alice, CreateOrganization { name: "acme".to_owned(), ..CreateOrganization::default() })
        .await
        .unwrap();
    fixture.services.orgs.add_member(&alice, &org, "bob", OrgRole::Member).await.unwrap();
    let receiver = Receiver::start(&[]).await;
    let webhooks = &fixture.services.webhooks;

    let as_bob = fixture.services.orgs.open(Some(&bob), "acme").await.unwrap();
    let refused = webhooks.create(&bob, HookTarget::Organization(&as_bob), input(&receiver.url, &["repository"], None)).await;
    assert!(matches!(refused, Err(ServiceError::Forbidden)), "members are not owners");

    let as_alice = fixture.services.orgs.open(Some(&alice), "acme").await.unwrap();
    let hook = webhooks.create(&alice, HookTarget::Organization(&as_alice), input(&receiver.url, &["repository"], None)).await.unwrap();
    assert_eq!(hook.owner, WebhookOwner::Organization(org.account.id));
    fixture.services.spawn_background();
    let ping = &receiver.wait_for(1).await[0];
    assert_eq!(ping.json()["organization"]["login"], "acme");

    let created = fixture.repo(&bob, "acme", "tools").await;
    let event = &receiver.wait_for(2).await[1];
    assert_eq!(event.header("X-Rbit-Event"), Some("repository"));
    let payload = event.json();
    assert_eq!(payload["action"], "created");
    assert_eq!(payload["repository"]["full_name"], "acme/tools");
    assert_eq!(payload["sender"]["login"], "bob");
    assert_eq!(payload["organization"]["login"], "acme");

    // Deleting reaches the organisation's hook although the repository's own are gone.
    let admin = fixture.services.repositories.open(Some(&alice), "acme", "tools").await.unwrap();
    fixture.services.repo_settings.delete(&alice, &admin, "acme/tools").await.unwrap();
    let deleted = receiver.wait_for(3).await[2].json();
    assert_eq!((deleted["action"].as_str(), deleted["repository"]["id"].as_i64()), (Some("deleted"), Some(created.repo.repo.id.0)));
    assert_eq!(deleted["sender"]["login"], "alice", "the deleting user is the sender");

    tokio::time::timeout(Duration::from_secs(12), fixture.services.shutdown()).await.unwrap();
}

#[tokio::test]
async fn management_is_validated_scoped_bounded_and_audited() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let mallory = fixture.user("mallory").await;
    let repo = fixture.repo(&alice, "alice", "demo").await;
    let other = fixture.repo(&alice, "alice", "other").await;
    let webhooks = &fixture.services.webhooks;
    let target = HookTarget::Repository(&repo);

    let invalid = |result: Result<_, ServiceError>| matches!(result, Err(ServiceError::Invalid(_)));
    assert!(invalid(webhooks.create(&alice, target, input("ftp://example.com/", &["push"], None)).await));
    assert!(invalid(webhooks.create(&alice, target, input("https://user:pw@example.com/", &["push"], None)).await));
    assert!(invalid(webhooks.create(&alice, target, input("https://example.com/", &["discussion"], None)).await));
    assert!(invalid(webhooks.create(&alice, target, input("https://example.com/", &[], None)).await));

    let hook = webhooks.create(&alice, target, input("https://example.com/a", &["push", "fork"], Some("s"))).await.unwrap();
    assert_eq!(hook.events, [WebhookEvent::Push, WebhookEvent::Fork]);
    assert!(matches!(webhooks.get(HookTarget::Repository(&other), hook.id).await, Err(ServiceError::NotFound)), "hooks are per repository");

    // Mallory may read the public repository but not administer it.
    let public = CreateRepository { name: "open".to_owned(), description: String::new(), visibility: Visibility::Public };
    fixture.services.repositories.create(&alice, public).await.unwrap();
    let as_mallory = fixture.services.repositories.open(Some(&mallory), "alice", "open").await.unwrap();
    assert!(matches!(webhooks.list(HookTarget::Repository(&as_mallory)).await, Err(ServiceError::Forbidden)));

    let mut change = input("https://example.com/b", &["release"], None);
    change.active = false;
    let changed = webhooks.update(&alice, target, hook.id, change.clone()).await.unwrap();
    assert!(changed.has_secret, "Keep keeps the secret");
    assert!(!changed.active);
    change.secret = SecretUpdate::Set(String::new());
    assert!(!webhooks.update(&alice, target, hook.id, change).await.unwrap().has_secret, "an empty secret clears it");

    for n in 1..MAX_HOOKS_PER_OWNER {
        webhooks.create(&alice, target, input(&format!("https://example.com/{n}"), &["push"], None)).await.unwrap();
    }
    let over = webhooks.create(&alice, target, input("https://example.com/over", &["push"], None)).await;
    assert!(matches!(over, Err(ServiceError::Invalid(message)) if message.contains("at most 20")));

    webhooks.delete(&alice, target, hook.id).await.unwrap();
    assert!(matches!(webhooks.delete(&alice, target, hook.id).await, Err(ServiceError::NotFound)));

    let actions: Vec<&str> =
        fixture.services.audit.recent_for(&alice, 50).await.unwrap().iter().map(|event| event.action.as_str()).collect();
    for action in ["webhook_created", "webhook_changed", "webhook_deleted"] {
        assert!(actions.contains(&action), "{action} is audited: {actions:?}");
    }
    let details: Vec<String> = fixture.services.audit.recent_for(&alice, 50).await.unwrap().into_iter().map(|event| event.detail).collect();
    assert!(details.iter().all(|detail| !detail.contains("https://")), "audit entries name the host, not the URL");
}

#[tokio::test]
async fn release_payloads_describe_the_release_and_forks_say_so() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let parent = fixture.repo(&alice, "alice", "demo").await;
    push_two_commits(&parent);
    let input_fork = ForkInput { owner: String::new(), name: "copy".to_owned() };
    fixture.services.forks.fork(&alice, &parent, input_fork).await.unwrap();
    let repo = fixture.services.repositories.open(Some(&alice), "alice", "copy").await.unwrap();
    let receiver = Receiver::start(&[]).await;
    let webhooks = &fixture.services.webhooks;
    webhooks.create(&alice, HookTarget::Repository(&repo), input(&receiver.url, &["release"], None)).await.unwrap();

    let releases = &fixture.services.releases;
    let release_input = ReleaseInput {
        tag_name: "v1.0".to_owned(),
        name: "First".to_owned(),
        notes: "Hello".to_owned(),
        is_draft: false,
        is_prerelease: true,
        target: "main".to_owned(),
    };
    let release = releases.create(&repo, &alice, release_input).await.unwrap();
    releases.upload_asset(&repo, &alice, release.id, "app.tar.gz", "application/gzip", &b"bytes"[..]).await.unwrap();
    let event = DomainEvent::ReleasePublished { repo: repo.repo.repo.id, release: release.id, actor: alice.id() };
    assert_eq!(webhooks.handle_event(&event).await.unwrap(), 1);
    webhooks.deliver_due(far_future()).await.unwrap();

    let requests = receiver.wait_for(2).await;
    let of = |event: &str| requests.iter().find(|request| request.header("X-Rbit-Event") == Some(event)).unwrap().json();
    let ping = of("ping");
    assert_eq!(ping["repository"]["fork"], true, "the repository object knows it is a fork");
    let payload = of("release");
    assert_eq!(payload["action"], "published");
    assert_eq!(payload["repository"]["fork"], true);
    let release_json = &payload["release"];
    assert_eq!(release_json["id"], release.id.0);
    assert_eq!(release_json["tag_name"], "v1.0");
    assert_eq!((release_json["name"].as_str(), release_json["body"].as_str()), (Some("First"), Some("Hello")));
    assert_eq!((release_json["draft"].as_bool(), release_json["prerelease"].as_bool()), (Some(false), Some(true)));
    assert!(release_json["published_at"].is_string());
    assert_eq!(release_json["author"]["login"], "alice");
    assert_eq!(release_json["html_url"], "https://git.example.com/alice/copy/releases/tag/v1.0");
    let asset = &release_json["assets"][0];
    assert_eq!((asset["name"].as_str(), asset["size"].as_u64()), (Some("app.tar.gz"), Some(5)));
    assert_eq!(asset["uploader"]["login"], "alice");
    assert_eq!(asset["browser_download_url"], "https://git.example.com/alice/copy/releases/download/v1.0/app.tar.gz");
}

#[tokio::test]
async fn internal_targets_are_refused_unless_allowed() {
    let fixture = Fixture::new(|config| config.webhooks.allow_private_addresses = false).await;
    let alice = fixture.user("alice").await;
    let repo = fixture.repo(&alice, "alice", "demo").await;
    let receiver = Receiver::start(&[]).await;
    let webhooks = &fixture.services.webhooks;
    let target = HookTarget::Repository(&repo);

    for url in [receiver.url.as_str(), "http://localhost:9/", "http://[::1]/", "http://169.254.169.254/latest", "http://10.0.0.1/"] {
        let refused = webhooks.create(&alice, target, input(url, &["push"], None)).await;
        assert!(matches!(&refused, Err(ServiceError::Invalid(message)) if message.contains("internal address")), "{url}: {refused:?}");
    }

    // Hooks that slipped past configuration (e.g. a name that now resolves internally)
    // are refused when delivering, without contacting the target and without retries.
    let port = receiver.url.trim_start_matches("http://127.0.0.1:").trim_end_matches("/hook");
    for url in [receiver.url.clone(), format!("http://localhost:{port}/hook")] {
        let new = NewWebhook {
            id: None,
            owner: WebhookOwner::Repository(repo.repo.repo.id),
            url: WebhookUrl::parse(&url).unwrap(),
            secret: None,
            events: vec![WebhookEvent::Push],
            active: true,
            created_by: None,
        };
        let hook = fixture.database.webhooks().create(new).await.unwrap();
        let ping = webhooks.ping(&alice, target, hook.id).await.unwrap();
        webhooks.deliver_due(far_future()).await.unwrap();
        let refused = webhooks.delivery(target, hook.id, ping.id).await.unwrap();
        assert_eq!((refused.status, refused.attempts), (DeliveryStatus::Failed, 1), "{url}");
        assert!(refused.error.as_deref().is_some_and(|error| error.contains("internal")), "{url}: {:?}", refused.error);
    }
    assert!(receiver.received().is_empty(), "nothing reached the internal listener");
}

#[tokio::test]
async fn disabled_webhooks_queue_nothing_in_the_background() {
    let fixture = Fixture::new(|config| config.webhooks.enabled = false).await;
    let alice = fixture.user("alice").await;
    let receiver = Receiver::start(&[]).await;
    let org = fixture
        .services
        .orgs
        .create(&alice, CreateOrganization { name: "acme".to_owned(), ..CreateOrganization::default() })
        .await
        .unwrap();
    let ctx = fixture.services.orgs.open(Some(&alice), org.name().as_str()).await.unwrap();
    let hook = fixture
        .services
        .webhooks
        .create(&alice, HookTarget::Organization(&ctx), input(&receiver.url, &["repository"], None))
        .await
        .unwrap();
    fixture.services.spawn_background();
    fixture.repo(&alice, "acme", "tools").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(receiver.received().is_empty());
    let log = fixture.services.webhooks.deliveries(HookTarget::Organization(&ctx), hook.id, 1, 10).await.unwrap();
    assert_eq!(log.deliveries.iter().map(|delivery| delivery.event).collect::<Vec<_>>(), [WebhookEvent::Ping], "only the ping, never sent");
    fixture.services.shutdown().await;
}
