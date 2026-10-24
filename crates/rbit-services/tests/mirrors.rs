//! Pull mirrors against real stores: a fetch from an upstream served over HTTP on an
//! ephemeral port (the fake forge's Git remote) is reported as a mirror's push.

use std::sync::Arc;

use rbit_core::{Config, PushSource, Visibility};
use rbit_services::{DomainEvent, Services, mirrors::CreateMirror};
use rbit_storage::{Database, NewUser};

use crate::fake_forge::FakeForge;

#[tokio::test]
async fn a_mirror_fetch_is_reported_as_a_mirror_push() {
    let forge = FakeForge::start("", None).await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.updates.check_for_updates = false;
    config.storage.data_dir = dir.path().join("data");
    config.mirrors.allow_private_addresses = true;
    std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = rbit_storage::KvStore::open(&config.storage.kv_path()).unwrap();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
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

    let input = CreateMirror {
        name: "rocket".to_owned(),
        description: String::new(),
        visibility: Visibility::Public,
        url: format!("{}/acme/rocket.git", forge.base),
        interval_minutes: None,
    };
    let created = services.mirrors.create(&alice, "alice", input).await.unwrap();
    let mut events = services.events.subscribe();
    services.mirrors.sync(created.repo.id).await.unwrap();
    let mirror = database.mirrors().find(created.repo.id).await.unwrap().unwrap();
    assert_eq!(mirror.last_error, None, "the fetch succeeded");

    let (repo, source, updates) = loop {
        if let DomainEvent::RepositoryPushed { repo, source, updates } = events.try_recv().expect("the fetch publishes a push") {
            break (repo, source, updates);
        }
    };
    assert_eq!((repo, source, source.pusher()), (created.repo.id, PushSource::Mirror, None));
    assert!(!source.starts_runs(), "an upstream's history never starts runs");
    assert!(updates.iter().any(|update| update.branch() == Some("main") && update.is_created()), "{updates:?}");
}
